/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useMemo } from 'react';
import { Info, Maximize2, Ratio, Film, HardDrive, Clock, Activity } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useThemeStore } from '../../../store/theme';
import request from '../../../utils/request';

interface MediaAttributes {
  width?: number;
  height?: number;
  aspectRatio?: string;
  duration?: number;
  format?: string;
  fileSize?: string;
  rawBytes?: number;
}

interface MediaPropertiesCardProps {
  url?: string | null;
  type?: 'video' | 'image' | 'auto';
  metadata?: {
    width?: number;
    height?: number;
    duration?: number;
  };
  style?: React.CSSProperties;
}

const COMMON_RATIOS: [number, string][] = [
  [16 / 9, '16:9'],
  [9 / 16, '9:16'],
  [4 / 3, '4:3'],
  [3 / 4, '3:4'],
  [1, '1:1'],
  [21 / 9, '21:9'],
  [3 / 2, '3:2'],
  [2 / 3, '2:3'],
  [4 / 5, '4:5'],
];

function calculateAspectRatio(w?: number, h?: number): string {
  if (!w || !h) return '-';
  const ratio = w / h;
  const match = COMMON_RATIOS.find(([r]) => Math.abs(ratio - r) < 0.03);
  if (match) return match[1];

  const gcd = (a: number, b: number): number => (b === 0 ? a : gcd(b, a % b));
  const d = gcd(Math.round(w), Math.round(h));
  const rw = Math.round(w / d);
  const rh = Math.round(h / d);
  return rw <= 25 && rh <= 25 ? `${rw}:${rh}` : `${ratio.toFixed(2)}:1`;
}

function detectFormat(url?: string, fallback: string = 'VIDEO'): string {
  if (!url) return fallback;
  try {
    const ext = new URL(url, window.location.href).pathname.split('.').pop()?.toUpperCase();
    if (ext && ext.length <= 5 && !ext.includes('/')) {
      return ext === 'JPEG' ? 'JPG' : ext;
    }
  } catch {}
  return fallback;
}

function formatDuration(sec?: number): string {
  if (!sec || isNaN(sec) || sec <= 0) return '-';
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return m > 0 ? `${m}m ${s}s` : `${sec.toFixed(1)}s`;
}

function formatFileSize(bytes?: number): string {
  if (!bytes || isNaN(bytes) || bytes <= 0) return '-';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function formatBitrate(bytes?: number, durationSec?: number): string {
  if (!bytes || !durationSec || isNaN(bytes) || isNaN(durationSec) || durationSec <= 0 || bytes <= 0) {
    return '-';
  }
  const kbps = Math.round((bytes * 8) / (durationSec * 1000));
  if (kbps >= 1000) {
    return `${kbps.toLocaleString()} kbps (${(kbps / 1000).toFixed(1)} Mbps)`;
  }
  return `${kbps} kbps`;
}

const MediaPropertiesCard: React.FC<MediaPropertiesCardProps> = ({
  url,
  type = 'auto',
  metadata,
  style,
}) => {
  const { t } = useTranslation();
  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';

  const colors = useMemo(() => ({
    border: isLight ? '#e4e4e7' : '#27272a',
    cardBg: isLight ? '#f8fafc' : '#111113',
    textPrimary: isLight ? '#09090b' : '#fafafa',
    textSecondary: isLight ? '#71717a' : '#a1a1aa',
  }), [isLight]);

  const resolvedType = useMemo(() => {
    if (type && type !== 'auto') return type;
    if (!url) return 'video';
    return url.match(/\.(png|jpe?g|webp|gif|svg|avif|bmp)($|\?)/i) ? 'image' : 'video';
  }, [type, url]);

  const [mediaAttrs, setMediaAttrs] = useState<MediaAttributes>({});

  useEffect(() => {
    if (!url) {
      setMediaAttrs({});
      return;
    }

    let isMounted = true;
    const format = detectFormat(url, resolvedType === 'image' ? 'IMAGE' : 'VIDEO');
    const initW = metadata?.width;
    const initH = metadata?.height;

    setMediaAttrs({
      format,
      width: initW,
      height: initH,
      duration: metadata?.duration,
      aspectRatio: initW && initH ? calculateAspectRatio(initW, initH) : undefined,
    });

    // 1. 媒体尺寸与时长探测
    if (resolvedType === 'video') {
      const v = document.createElement('video');
      v.preload = 'metadata';
      v.src = url;
      v.onloadedmetadata = () => {
        if (!isMounted) return;
        const w = v.videoWidth;
        const h = v.videoHeight;
        const dur = v.duration;
        setMediaAttrs((prev) => ({
          ...prev,
          width: prev.width ?? w,
          height: prev.height ?? h,
          duration: prev.duration ?? dur,
          aspectRatio: prev.aspectRatio ?? calculateAspectRatio(w, h),
        }));
      };
    } else {
      const img = new window.Image();
      img.src = url;
      img.onload = () => {
        if (!isMounted) return;
        const w = img.naturalWidth || img.width;
        const h = img.naturalHeight || img.height;
        setMediaAttrs((prev) => ({
          ...prev,
          width: prev.width ?? w,
          height: prev.height ?? h,
          aspectRatio: prev.aspectRatio ?? calculateAspectRatio(w, h),
        }));
      };
    }

    // 2. 文件大小探测
    const updateSize = (bytes: number) => {
      if (bytes > 0 && isMounted) {
        setMediaAttrs((prev) => ({
          ...prev,
          fileSize: formatFileSize(bytes),
          rawBytes: bytes,
        }));
      }
    };

    if (url.startsWith('data:')) {
      const base64Str = url.split(',')[1] || '';
      const bytes = Math.round((base64Str.length * 3) / 4);
      updateSize(bytes);
      return;
    }

    if (url.startsWith('blob:')) {
      fetch(url)
        .then((r) => r.blob())
        .then((b) => updateSize(b.size))
        .catch(() => {});
      return;
    }

    const detectSize = async () => {
      // 性能条目
      try {
        const entries = performance.getEntriesByName(url);
        const last = entries[entries.length - 1] as PerformanceResourceTiming | undefined;
        const sz = last?.encodedBodySize || last?.decodedBodySize || last?.transferSize;
        if (sz && sz > 0) {
          updateSize(sz);
          return;
        }
      } catch {}

      // HEAD / Range 轻量探测
      const getBytesFromRes = (res: Response): number | null => {
        const cr = res.headers.get('content-range');
        if (cr) {
          const total = parseInt(cr.split('/')[1] || '', 10);
          if (!isNaN(total) && total > 0) return total;
        }
        const cl = parseInt(res.headers.get('content-length') || '', 10);
        return !isNaN(cl) && cl > 0 ? cl : null;
      };

      try {
        const headRes = await fetch(url, { method: 'HEAD', mode: 'cors' });
        const bytes = getBytesFromRes(headRes);
        if (bytes) {
          updateSize(bytes);
          return;
        }
      } catch {}

      try {
        const rangeRes = await fetch(url, { method: 'GET', headers: { Range: 'bytes=0-0' }, mode: 'cors' });
        const bytes = getBytesFromRes(rangeRes);
        if (bytes) {
          updateSize(bytes);
          return;
        }
      } catch {}

      // 后端中转兜底
      try {
        const data = await request.get('/playground/download-file', {
          params: { url },
          responseType: 'blob',
          timeout: 10000,
          ...({ skipErrorHandler: true } as any),
        });
        if (data instanceof Blob && data.size > 0 && !data.type?.includes('json')) {
          updateSize(data.size);
        }
      } catch {}
    };

    detectSize();

    return () => {
      isMounted = false;
    };
  }, [url, resolvedType, metadata?.width, metadata?.height, metadata?.duration]);

  if (!url) return null;

  const cardStyle: React.CSSProperties = {
    background: colors.cardBg,
    border: `1px solid ${colors.border}`,
    borderRadius: 8,
    padding: '8px 12px',
  };

  const labelStyle: React.CSSProperties = {
    fontSize: 11,
    color: colors.textSecondary,
    display: 'flex',
    alignItems: 'center',
    gap: 4,
  };

  const valueStyle: React.CSSProperties = {
    fontSize: 13,
    fontWeight: 600,
    color: colors.textPrimary,
    marginTop: 3,
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
  };

  const items = [
    {
      key: 'dim',
      icon: Maximize2,
      label: t('task_logs.prop_dimensions', '分辨率'),
      value: mediaAttrs.width && mediaAttrs.height ? `${mediaAttrs.width} × ${mediaAttrs.height}` : '-',
    },
    {
      key: 'ratio',
      icon: Ratio,
      label: t('task_logs.prop_aspect_ratio', '宽高比'),
      value: mediaAttrs.aspectRatio || '-',
    },
    {
      key: 'format',
      icon: Film,
      label: t('task_logs.prop_format', '文件格式'),
      value: mediaAttrs.format || '-',
    },
    {
      key: 'size',
      icon: HardDrive,
      label: t('task_logs.prop_file_size', '文件大小'),
      value: mediaAttrs.fileSize || '-',
    },
    ...(resolvedType === 'video'
      ? [
          {
            key: 'duration',
            icon: Clock,
            label: t('task_logs.prop_duration', '视频时长'),
            value: formatDuration(mediaAttrs.duration),
          },
          {
            key: 'bitrate',
            icon: Activity,
            label: t('task_logs.prop_bitrate', '视频码率'),
            value: formatBitrate(mediaAttrs.rawBytes, mediaAttrs.duration),
          },
        ]
      : []),
  ];

  return (
    <div style={{ marginTop: 12, ...style }}>
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          color: colors.textSecondary,
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          marginBottom: 10,
          display: 'flex',
          alignItems: 'center',
          gap: 5,
        }}
      >
        <Info size={13} />
        <span>{t('task_logs.media_properties', '媒体基本属性')}</span>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 8 }}>
        {items.map(({ key, icon: Icon, label, value }) => (
          <div key={key} style={cardStyle}>
            <div style={labelStyle}>
              <Icon size={12} />
              <span>{label}</span>
            </div>
            <div style={valueStyle}>{value}</div>
          </div>
        ))}
      </div>
    </div>
  );
};

export default MediaPropertiesCard;
