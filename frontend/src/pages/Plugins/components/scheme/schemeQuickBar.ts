/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import type { SchemeParam } from './types';
import i18n from '../../../../i18n';
import { isCountParam, isDurationParam, isSingleCountParam, DEFAULT_OPTION_LABELS, formatChoiceLabel, formatParamLabel } from './schemeParamUtils';
import { pickSchemeOption } from './generationParams';

export const SCHEME_QUICK_BAR_MAX = 3;

export const SCHEME_QUICK_BAR_HELP =
  `勾选后，该参数会出现在独立图片/视频/音频生成页提示词下方的快捷栏。同一方案（含该模型微调）最多开启 ${SCHEME_QUICK_BAR_MAX} 个；左右顺序与参数列表一致。仅「单选 / 下拉」且已填写选项的参数可开启。`;

/** 快捷栏只适合做成选项条的单选 / 下拉，且排除生成数量固定为 1 的参数 */
export function isQuickBarEligible(param: SchemeParam): boolean {
  if (isSingleCountParam(param)) return false;
  return (
    (param.type === 'radio' || param.type === 'select') &&
    Array.isArray(param.options) &&
    param.options.length > 0
  );
}

const QUICK_OPTION_I18N_KEYS: Record<string, string> = {
  opaque: 'opt_opaque',
  transparent: 'opt_transparent',
  standard: 'opt_standard',
  fast: 'opt_fast',
  auto: 'opt_auto',
  adaptive: 'aspect_adaptive',
  high: 'opt_high',
  ultra: 'opt_ultra',
  medium: 'opt_standard',
  low: 'opt_low',
  quality: 'opt_quality',
  speed: 'opt_speed',
  portrait: 'opt_portrait',
  landscape: 'opt_landscape',
};

const CHINESE_QUICK_OPTION_I18N_KEYS: Record<string, string> = {
  '不透明': 'opt_opaque',
  '透明': 'opt_transparent',
  '标准': 'opt_standard',
  '快速': 'opt_fast',
  '智能': 'opt_auto',
  '自适应': 'aspect_adaptive',
  '高清': 'opt_high',
  '超清': 'opt_ultra',
  '画质优先': 'opt_quality',
  '速度优先': 'opt_speed',
  '人像': 'opt_portrait',
  '风景': 'opt_landscape',
};

/** 参数选项展示文案（优先 option_labels，其次内置中文字典，时长加 s，数量加张） */
export function formatParamOptionLabel(param: SchemeParam, val: any): string {
  if (val == null || val === '') return formatParamLabel(param);
  const str = String(val).trim();
  const lower = str.toLowerCase();
  const curLang = i18n.language || 'zh';
  const isSimplified = curLang === 'zh' || curLang === 'zh-CN';

  // 1. 优先自定义 option_labels；背景/提示词优化的内置中文走属性框同一套语言
  if (param.option_labels) {
    const custom = param.option_labels[str] || param.option_labels[lower];
    if (custom) {
      const paramKey = (param.key || '').toLowerCase();
      if (paramKey === 'background' || paramKey === 'optimize_prompt_mode') {
        return formatChoiceLabel(param, val);
      }
      return custom;
    }
  }

  // 针对 quality 画质参数的定制中文对照与历史值回退
  if ((param.key || '').toLowerCase() === 'quality') {
    if (lower === 'low' || str === '常用画质' || str === '流畅') return i18n.t('playground_2026:quality_low', '常用画质');
    if (lower === 'medium' || str === '高画质' || str === '标准') return i18n.t('playground_2026:quality_medium', '高画质');
    if (lower === 'high' || str === '高精细画质' || str === '高清') return i18n.t('playground_2026:quality_high', '高精细画质');
  }

  // 2. 时长
  if (isDurationParam(param)) {
    if (lower === '-1' || lower === 'auto' || lower === 'adaptive' || str === '智能' || str === '自适应') {
      return i18n.t('playground_2026:smart_duration', '智能时长');
    }
    if (/^\d+(\.\d+)?$/.test(str)) {
      return `${str}s`;
    }
  }

  // 3. 数量
  if (isCountParam(param)) {
    return formatChoiceLabel(param, val);
  }

  // 4. 中文选项反向匹配 (非简体中文)
  if (!isSimplified && CHINESE_QUICK_OPTION_I18N_KEYS[str]) {
    return i18n.t(`playground_2026:${CHINESE_QUICK_OPTION_I18N_KEYS[str]}`, str);
  }

  // 5. 内置常用选项对照 (支持多语言)
  if (QUICK_OPTION_I18N_KEYS[lower]) {
    return i18n.t(`playground_2026:${QUICK_OPTION_I18N_KEYS[lower]}`, DEFAULT_OPTION_LABELS[lower] || str);
  }
  if (DEFAULT_OPTION_LABELS[lower]) {
    return DEFAULT_OPTION_LABELS[lower];
  }

  return str;
}

/** 底栏属性摘要：后台勾了快捷栏的参数；一个都没勾则用第一个可展示参数 */
export function getShortcutSummaryParams(
  displayParams: SchemeParam[],
  opts?: { skipFirstFallback?: boolean },
): SchemeParam[] {
  const quick = displayParams
    .filter((p) => !!p.quick && !isSingleCountParam(p))
    .slice(0, SCHEME_QUICK_BAR_MAX);
  if (quick.length > 0) return quick;
  if (opts?.skipFirstFallback) return [];
  const first = displayParams.find((p) => !isSingleCountParam(p));
  return first ? [first] : [];
}

export function formatShortcutSummary(
  params: SchemeParam[],
  values: Record<string, any>,
): string {
  return params
    .map((p) => {
      const raw = values[p.key] ?? p.default;
      const v = p.options?.length ? pickSchemeOption(p.options, raw, p.default) : raw;
      return formatParamOptionLabel(p, v);
    })
    .filter((s) => !!s)
    .join(' | ');
}

type QuickBarOverrideBundle = {
  modify?: Record<string, { quick?: boolean } & Record<string, unknown>>;
  remove?: string[];
  add?: Array<{ quick?: boolean } & Record<string, unknown>>;
};

/** 方案参数 + 模型微调合并后，已勾选快捷栏的数量 */
export function countQuickBarEnabled(
  schemeParams: Array<{ key?: string; quick?: boolean }>,
  overrides?: QuickBarOverrideBundle | null,
  skip?: { key?: string; addIndex?: number },
): number {
  const removes = new Set(overrides?.remove || []);
  const modifies = overrides?.modify || {};
  let n = 0;
  for (const p of schemeParams) {
    const key = p.key || '';
    if (!key || removes.has(key)) continue;
    if (skip?.key && key === skip.key) continue;
    const merged = { ...p, ...(modifies[key] || {}) };
    if (merged.quick) n++;
  }
  (overrides?.add || []).forEach((ap, i) => {
    if (skip?.addIndex === i) return;
    if (ap?.quick) n++;
  });
  return n;
}
