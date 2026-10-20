#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TokensByte (TkeApi) 生产级零停机热更新并发流量探针与无损验证工具
用于在执行 hot-update.sh 期间持续注入高并发常规请求与长流式请求，
高精度监测切流窗口期是否有任何 502、连接重置、请求中断或丢包。
"""

import argparse
import json
import os
import signal
import sys
import threading
import time
import urllib.request
import urllib.error
from collections import defaultdict
from dataclasses import dataclass, field
from typing import Dict, List, Optional


@dataclass
class RequestResult:
    timestamp: float
    duration_ms: float
    status_code: int
    is_stream: bool
    error: Optional[str] = None


class TrafficProbe:
    def __init__(self, base_url: str, concurrency: int = 10, stream_concurrency: int = 2, qps: int = 40):
        self.base_url = base_url.rstrip("/")
        self.concurrency = concurrency
        self.stream_concurrency = stream_concurrency
        self.interval = 1.0 / max(qps, 1) if qps > 0 else 0.02
        self.running = False
        self.results: List[RequestResult] = []
        self.lock = threading.Lock()
        self.start_time: float = 0
        self.end_time: float = 0

    def _send_probe_req(self, is_stream: bool = False):
        url = f"{self.base_url}/api/health"
        t0 = time.time()
        status_code = 0
        err_msg = None

        try:
            req = urllib.request.Request(
                url,
                headers={
                    "User-Agent": "TokensByte-HotUpdate-Probe/1.0",
                    "Accept": "*/*",
                }
            )
            # 长连接模拟：稍微延长读超时
            timeout = 30.0 if is_stream else 5.0
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                status_code = resp.status
                _ = resp.read(1024)
                if is_stream:
                    # 模拟长请求维持连接
                    time.sleep(1.5)
        except urllib.error.HTTPError as e:
            status_code = e.code
            err_msg = f"HTTPError {e.code}: {e.reason}"
        except urllib.error.URLError as e:
            status_code = 599
            err_msg = f"URLError: {e.reason}"
        except Exception as e:
            status_code = 598
            err_msg = f"Exception: {type(e).__name__}: {str(e)}"

        duration_ms = (time.time() - t0) * 1000.0
        res = RequestResult(
            timestamp=t0,
            duration_ms=duration_ms,
            status_code=status_code,
            is_stream=is_stream,
            error=err_msg
        )
        with self.lock:
            self.results.append(res)

    def _worker_loop(self):
        while self.running:
            self._send_probe_req(is_stream=False)
            if self.interval > 0:
                time.sleep(self.interval)

    def _stream_worker_loop(self):
        while self.running:
            self._send_probe_req(is_stream=True)
            time.sleep(0.5)

    def start(self):
        self.running = True
        self.start_time = time.time()
        self.threads: List[threading.Thread] = []

        # 启动常规高频探针
        for i in range(self.concurrency):
            t = threading.Thread(target=self._worker_loop, name=f"probe-worker-{i}", daemon=True)
            t.start()
            self.threads.append(t)

        # 启动长连接模拟探针
        for i in range(self.stream_concurrency):
            t = threading.Thread(target=self._stream_worker_loop, name=f"stream-worker-{i}", daemon=True)
            t.start()
            self.threads.append(t)

    def stop(self):
        self.running = False
        self.end_time = time.time()
        for t in self.threads:
            t.join(timeout=2.0)

    def generate_report(self) -> Dict:
        with self.lock:
            res_copy = list(self.results)

        total_reqs = len(res_copy)
        if total_reqs == 0:
            return {"error": "无测试请求产生"}

        success_reqs = [r for r in res_copy if 200 <= r.status_code < 400]
        failed_reqs = [r for r in res_copy if r.status_code >= 400 or r.status_code == 0]
        stream_reqs = [r for r in res_copy if r.is_stream]
        stream_failed = [r for r in stream_reqs if r.status_code >= 400 or r.status_code == 0]

        durations = sorted([r.duration_ms for r in success_reqs]) if success_reqs else [0.0]
        p50 = durations[int(len(durations) * 0.50)]
        p95 = durations[int(len(durations) * 0.95)]
        p99 = durations[int(len(durations) * 0.99)]
        max_lat = durations[-1]
        min_lat = durations[0]

        # 按秒聚合时序状态
        time_buckets = defaultdict(lambda: {"total": 0, "success": 0, "failed": 0, "status": defaultdict(int)})
        base_t = int(self.start_time)
        for r in res_copy:
            sec = int(r.timestamp) - base_t
            b = time_buckets[sec]
            b["total"] += 1
            if 200 <= r.status_code < 400:
                b["success"] += 1
            else:
                b["failed"] += 1
            b["status"][r.status_code] += 1

        duration_total = max(self.end_time - self.start_time, 0.001)
        actual_qps = total_reqs / duration_total

        return {
            "summary": {
                "base_url": self.base_url,
                "duration_seconds": round(duration_total, 2),
                "total_requests": total_reqs,
                "success_requests": len(success_reqs),
                "failed_requests": len(failed_reqs),
                "success_rate_percent": round(len(success_reqs) / total_reqs * 100, 3),
                "avg_qps": round(actual_qps, 1),
                "stream_total": len(stream_reqs),
                "stream_interrupted": len(stream_failed),
            },
            "latency_ms": {
                "min": round(min_lat, 2),
                "p50": round(p50, 2),
                "p95": round(p95, 2),
                "p99": round(p99, 2),
                "max": round(max_lat, 2),
            },
            "error_breakdown": [
                {"status_code": r.status_code, "error": r.error, "timestamp_rel": round(r.timestamp - self.start_time, 2)}
                for r in failed_reqs[:10]
            ],
            "timeline_seconds": [
                {
                    "second": s,
                    "total": time_buckets[s]["total"],
                    "success": time_buckets[s]["success"],
                    "failed": time_buckets[s]["failed"],
                    "status_codes": dict(time_buckets[s]["status"])
                }
                for s in sorted(time_buckets.keys())
            ]
        }


def print_cli_summary(report: Dict):
    s = report["summary"]
    lat = report["latency_ms"]
    print("\n" + "=" * 65)
    print("      🚀 TokensByte 热更新零停机/无损切流压测验证报告")
    print("=" * 65)
    print(f"目标地址:            {s['base_url']}")
    print(f"测试持续时长:        {s['duration_seconds']} 秒")
    print(f"探针请求总量:        {s['total_requests']:,} 次 (实际均值 {s['avg_qps']} QPS)")
    print(f"成功响应 (HTTP 200): {s['success_requests']:,} 次 ({s['success_rate_percent']}%)")
    print(f"异常/502/断连中断:   {s['failed_requests']:,} 次")
    print(f"长连接/流式中断:     {s['stream_interrupted']} / {s['stream_total']} 次")
    print("-" * 65)
    print(f"响应延迟统计:        Min: {lat['min']}ms | P50: {lat['p50']}ms | P95: {lat['p95']}ms | P99: {lat['p99']}ms | Max: {lat['max']}ms")
    print("-" * 65)

    if s["failed_requests"] == 0 and s["stream_interrupted"] == 0:
        print("✅ 验证结论: 零停机热更新完美通过！切流期间 0 丢包、0 断连、0 报错！")
    else:
        print(f"⚠️  验证结论: 检测到 {s['failed_requests']} 个异常请求，详情如下：")
        for err in report.get("error_breakdown", []):
            print(f"   · 第 {err['timestamp_rel']}s: Code={err['status_code']}, {err['error']}")
    print("=" * 65 + "\n")


def main():
    parser = argparse.ArgumentParser(description="TokensByte 热更新并发无损验证工具")
    parser.add_argument("--url", default="http://127.0.0.1:3000", help="测试目标基础 URL (默认: http://127.0.0.1:3000)")
    parser.add_argument("--duration", type=int, default=30, help="测试持续秒数 (默认: 30)")
    parser.add_argument("--concurrency", type=int, default=15, help="并发常规探针线程数 (默认: 15)")
    parser.add_argument("--stream-concurrency", type=int, default=3, help="模拟长连接/流式探针数 (默认: 3)")
    parser.add_argument("--qps", type=int, default=50, help="目标单线程请求频率")
    parser.add_argument("--output", default="", help="保存报告为 JSON 文件路径")
    parser.add_argument("--daemon", action="store_true", help="守护进程模式：持续运行直到接收 SIGINT/SIGTERM")
    args = parser.parse_args()

    probe = TrafficProbe(
        base_url=args.url,
        concurrency=args.concurrency,
        stream_concurrency=args.stream_concurrency,
        qps=args.qps
    )

    def handle_signal(sig, frame):
        print("\n[Probe] 接收到终止信号，正在收集最终统计并生成报告...")
        probe.stop()
        report = probe.generate_report()
        print_cli_summary(report)
        if args.output:
            with open(args.output, "w", encoding="utf-8") as f:
                json.dump(report, f, indent=2, ensure_ascii=False)
            print(f"📄 详细 JSON 报告已保存至: {args.output}")
        sys.exit(0 if report["summary"]["failed_requests"] == 0 else 1)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    print(f"🚀 正在向 {args.url} 启动流量探针 ({args.concurrency} 并发请求 + {args.stream_concurrency} 模拟流式长连接)...")
    probe.start()

    if args.daemon:
        print("[Probe] 守护模式运行中，按 Ctrl+C 或通过 SIGTERM 停止并输出报告...")
        while True:
            time.sleep(1)
    else:
        time.sleep(args.duration)
        probe.stop()
        report = probe.generate_report()
        print_cli_summary(report)
        if args.output:
            with open(args.output, "w", encoding="utf-8") as f:
                json.dump(report, f, indent=2, ensure_ascii=False)
            print(f"📄 详细 JSON 报告已保存至: {args.output}")
        sys.exit(0 if report["summary"]["failed_requests"] == 0 else 1)


if __name__ == "__main__":
    main()
