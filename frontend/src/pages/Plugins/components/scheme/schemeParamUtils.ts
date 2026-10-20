/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import type { SchemeParam } from './types';
import i18n from '../../../../i18n';

export const isCountParam = (param: SchemeParam): boolean => {
  const k = (param.key || '').toLowerCase();
  const label = (param.label || '').toLowerCase();
  return (
    k === 'n' ||
    k === 'batch_size' ||
    k === 'num_images' ||
    k === 'image_count' ||
    k === 'count' ||
    k === 'quantity' ||
    k === 'generate_count' ||
    k === 'candidate_count' ||
    k === 'variations' ||
    label === 'image count' ||
    label.includes('image count') ||
    label === 'count' ||
    label.includes('quantity') ||
    label.includes('batch size') ||
    label.includes('number of images') ||
    label === '生成数量' ||
    label.includes('生成数量') ||
    label.includes('图片数量') ||
    label.includes('生成张数') ||
    label.includes('变体数量')
  );
};

export const isDurationParam = (param?: SchemeParam | null): boolean => {
  if (!param) return false;
  const k = (param.key || '').toLowerCase();
  const label = (param.label || '').toLowerCase();
  return (
    k.includes('duration') ||
    k.includes('time') ||
    k.includes('second') ||
    label.includes('时长') ||
    label.includes('时间') ||
    label.includes('秒')
  );
};

/**
 * 判断生成数量参数是否固定为 1（或最大只能为 1 / 仅 1 个选项 1）
 * 当生成数量为 1 时，不需要在前端设置面板与快捷栏中显示生成数量调节
 */
export const isSingleCountParam = (param?: SchemeParam | null): boolean => {
  if (!param || !isCountParam(param)) return false;

  // 1. 若配置了 options 列表
  if (Array.isArray(param.options)) {
    if (param.options.length <= 1) return true;
    const nums = param.options
      .map((o) => {
        if (typeof o === 'number') return o;
        const m = String(o).match(/\d+/);
        return m ? parseInt(m[0], 10) : NaN;
      })
      .filter((n) => !Number.isNaN(n));
    if (nums.length > 0 && Math.max(...nums) <= 1) return true;
  }

  // 2. 若配置了数字区间 max <= 1
  if (param.type === 'number' || param.type === 'slider') {
    if (param.max !== undefined && param.max <= 1) return true;
  }

  // 3. 未配置 options 且 max 未显式指定为 > 1（例如仅默认 1）
  if ((!param.options || param.options.length === 0) && (param.max === undefined || param.max <= 1)) {
    return true;
  }

  return false;
};

/** 常用英文选项内置中文对照字典 */
export const DEFAULT_OPTION_LABELS: Record<string, string> = {
  opaque: '不透明',
  transparent: '透明',
  png: 'PNG',
  jpeg: 'JPEG',
  jpg: 'JPG',
  webp: 'WebP',
  mp4: 'MP4',
  mov: 'MOV',
  standard: '标准',
  fast: '快速',
  auto: '智能',
  adaptive: '自适应',
  high: '高清',
  ultra: '超清',
  medium: '标准',
  low: '流畅',
  quality: '画质优先',
  speed: '速度优先',
  portrait: '人像',
  landscape: '风景',
};

const OPTION_I18N_KEYS: Record<string, string> = {
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

const CHINESE_OPTION_I18N_KEYS: Record<string, string> = {
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

/** 背景 / 提示词优化的内置中文（不透明、标准等）随界面语言显示；其它自定义文案保持原样 */
const BUILTIN_CHOICE_PARAM_KEYS = new Set(['background', 'optimize_prompt_mode']);

function localizeBuiltinChoice(param: SchemeParam, stored: string, isSimplified: boolean): string | null {
  if (isSimplified) return null;
  const paramKey = (param.key || '').trim().toLowerCase();
  if (!BUILTIN_CHOICE_PARAM_KEYS.has(paramKey)) return null;
  const i18nKey = CHINESE_OPTION_I18N_KEYS[stored];
  if (!i18nKey) return null;
  return i18n.t(`playground_2026:${i18nKey}`, stored);
}

/** 数量参数选项文案格式化（支持单复数与多语言） */
export const formatCountChoice = (param: SchemeParam, opt: string | number, isSimplified: boolean): string => {
  const s = String(opt).trim();
  const m = s.match(/\d+/);
  if (!m) {
    return s;
  }
  const n = parseInt(m[0], 10);
  if (Number.isNaN(n)) return s;

  if (isSimplified) {
    return `${n} 张`;
  }

  const curLang = (i18n.language || 'zh').toLowerCase();
  if (curLang === 'zh-tw' || curLang === 'zh-hk') {
    return i18n.t('playground_2026:count_image_plural', { count: n, defaultValue: `${n} 張` });
  }
  if (curLang.startsWith('ja')) {
    return i18n.t('playground_2026:count_image_plural', { count: n, defaultValue: `${n} 枚` });
  }
  if (curLang.startsWith('ko')) {
    return i18n.t('playground_2026:count_image_plural', { count: n, defaultValue: `${n} 장` });
  }
  if (curLang.startsWith('vi')) {
    return i18n.t('playground_2026:count_image_plural', { count: n, defaultValue: `${n} tấm` });
  }

  // English & other fallback
  if (n === 1) {
    return i18n.t('playground_2026:count_image_one', { count: 1, defaultValue: '1 image' });
  }
  return i18n.t('playground_2026:count_image_plural', { count: n, defaultValue: `${n} images` });
};

/** 选项文案：优先 option_labels，其次内置中文字典，时长 -1/数字加单位，其余跟方案 unit */
export const formatChoiceLabel = (param: SchemeParam, opt: string | number): string => {
  const s = String(opt).trim();
  const lower = s.toLowerCase();
  const curLang = i18n.language || 'zh';
  const isSimplified = curLang === 'zh' || curLang === 'zh-CN';

  // 1. 优先使用自定义配置的 option_labels 映射
  if (param.option_labels) {
    const custom = param.option_labels[s] || param.option_labels[lower];
    if (custom) {
      if ((param.key || '').toLowerCase() === 'quality' && !isSimplified) {
        const qk =
          custom === '常用画质' || custom === '流畅' || custom.toLowerCase() === 'low'
            ? 'quality_low'
            : custom === '高画质' || custom === '标准' || custom.toLowerCase() === 'medium'
              ? 'quality_medium'
              : custom === '高精细画质' || custom === '高清' || custom.toLowerCase() === 'high'
                ? 'quality_high'
                : '';
        if (qk) return i18n.t(`playground_2026:${qk}`, custom);
      }
      return localizeBuiltinChoice(param, custom, isSimplified) || custom;
    }
  }

  // 2. 针对 quality 画质参数的定制中文对照与历史值回退
  if ((param.key || '').toLowerCase() === 'quality') {
    if (lower === 'low' || s === '常用画质' || s === '流畅') return i18n.t('playground_2026:quality_low', '常用画质');
    if (lower === 'medium' || s === '高画质' || s === '标准') return i18n.t('playground_2026:quality_medium', '高画质');
    if (lower === 'high' || s === '高精细画质' || s === '高清') return i18n.t('playground_2026:quality_high', '高精细画质');
  }

  // 3. 特殊参数逻辑：时长智能 / 数量单位
  if (isDurationParam(param)) {
    if (lower === '-1' || lower === 'auto' || lower === 'adaptive' || s === '智能' || s === '自适应') {
      return i18n.t('playground_2026:opt_auto', '智能');
    }
    if (/^\d+(\.\d+)?$/.test(s)) return `${s}s`;
  }

  if (isCountParam(param)) {
    return formatCountChoice(param, opt, isSimplified);
  }

  // 4. 中文选项反向匹配 (当非简体中文时)
  if (!isSimplified && CHINESE_OPTION_I18N_KEYS[s]) {
    const matched = i18n.t(`playground_2026:${CHINESE_OPTION_I18N_KEYS[s]}`, s);
    return param.unit ? `${matched} ${param.unit}` : matched;
  }

  // 5. 内置常用英文选项字典匹配 (支持多语言切换)
  if (OPTION_I18N_KEYS[lower]) {
    const matched = i18n.t(`playground_2026:${OPTION_I18N_KEYS[lower]}`, DEFAULT_OPTION_LABELS[lower]);
    return param.unit ? `${matched} ${param.unit}` : matched;
  }
  if (DEFAULT_OPTION_LABELS[lower]) {
    const matched = DEFAULT_OPTION_LABELS[lower];
    return param.unit ? `${matched} ${param.unit}` : matched;
  }

  if (param.unit) {
    let unit = param.unit;
    if (!isSimplified) {
      if (unit === '张' || unit === '張') {
        const langLower = curLang.toLowerCase();
        if (langLower === 'zh-tw' || langLower === 'zh-hk') unit = '張';
        else if (langLower.startsWith('ja')) unit = '枚';
        else if (langLower.startsWith('ko')) unit = '장';
        else if (langLower.startsWith('vi')) unit = 'tấm';
        else unit = s === '1' ? 'image' : 'images';
      } else if (unit === '秒') {
        unit = 's';
      }
    }
    return `${s} ${unit}`;
  }

  return s;
};

/** 常用英文参数名称内置中文对照字典 */
export const DEFAULT_PARAM_LABELS: Record<string, string> = {
  n: '生成数量',
  batch_size: '生成数量',
  num_images: '生成数量',
  image_count: '生成数量',
  count: '生成数量',
  quantity: '生成数量',
  generate_count: '生成数量',
  candidate_count: '生成数量',
  'image count': '生成数量',
  'prompt enhancer': '提示词增强',
  prompt_enhancer: '提示词增强',
  variations: '变体数量',
  variation: '变体数量',
  resolution: '分辨率',
  quality: '画质',
  watermark: '水印',
  output_format: '输出格式',
  background: '背景',
  layer_decomposition: '自动拆分图层',
  optimize_prompt_mode: '提示词优化',
  ratio: '画面比例',
  aspect_ratio: '画面比例',
  duration: '生成时长',
  return_last_frame: '返回最后一帧',
  generate_audio: '生成音频',
  web_search: '联网搜索',
  prompt_extend: '提示词扩写',
  size: '图片尺寸',
  style: '画面风格',
  seed: '随机种子',
};

const PARAM_LABEL_I18N_KEYS: Record<string, string> = {
  n: 'param_count',
  batch_size: 'param_count',
  num_images: 'param_count',
  image_count: 'param_count',
  count: 'param_count',
  quantity: 'param_count',
  generate_count: 'param_count',
  candidate_count: 'param_count',
  'image count': 'param_count',
  'prompt enhancer': 'param_prompt_enhancer',
  prompt_enhancer: 'param_prompt_enhancer',
  variations: 'param_variations',
  variation: 'param_variations',
  resolution: 'param_resolution',
  quality: 'param_quality',
  watermark: 'param_watermark',
  output_format: 'param_output_format',
  background: 'param_background',
  layer_decomposition: 'param_layer_decomposition',
  optimize_prompt_mode: 'param_optimize_prompt_mode',
  ratio: 'param_ratio',
  aspect_ratio: 'param_ratio',
  duration: 'param_duration',
  return_last_frame: 'param_return_last_frame',
  generate_audio: 'param_generate_audio',
  web_search: 'param_web_search',
  prompt_extend: 'param_prompt_extend',
  size: 'param_size',
  style: 'param_style',
  seed: 'param_seed',
};

const CHINESE_PARAM_I18N_KEYS: Record<string, string> = {
  '生成数量': 'param_count',
  '图片数量': 'param_count',
  '生成张数': 'param_count',
  '提示词增强': 'param_prompt_enhancer',
  '变体数量': 'param_variations',
  '分辨率': 'param_resolution',
  '画质': 'param_quality',
  '水印': 'param_watermark',
  '输出格式': 'param_output_format',
  '背景': 'param_background',
  '自动拆分图层': 'param_layer_decomposition',
  '提示词优化': 'param_optimize_prompt_mode',
  '画面比例': 'param_ratio',
  '生成时长': 'param_duration',
  '视频时长': 'param_duration',
  '输出分辨率': 'param_resolution',
  '返回最后一帧': 'param_return_last_frame',
  '生成音频': 'param_generate_audio',
  '联网搜索': 'param_web_search',
  '提示词扩写': 'param_prompt_extend',
  '图片尺寸': 'param_size',
  '画面风格': 'param_style',
  '随机种子': 'param_seed',
};

/** 常用参数默认中文功能说明/描述文案字典（描述菜单） */
export const DEFAULT_PARAM_HINTS: Record<string, string> = {
  n: '单次生成的图片数量',
  batch_size: '单次生成的图片数量',
  count: '单次生成的图片数量',
  image_count: '单次生成的图片数量',
  quantity: '单次生成的图片数量',
  generate_count: '单次生成的图片数量',
  prompt_enhancer: '开启后智能优化与润色提示词，丰富画面细节',
  'prompt enhancer': '开启后智能优化与润色提示词，丰富画面细节',
  variations: '单次生成的图片变体数量 (1-4)',
  variation: '单次生成的图片变体数量 (1-4)',
  resolution: '设置生成图片的清晰度规格 (1K/2K/4K)',
  quality: '设置生成画面的精细度质量 (常用画质/高画质/高精细画质)',
  layer_decomposition: '把参考图拆成底图 + 透明图层；提示词可空',
};

const PARAM_HINT_I18N_KEYS: Record<string, string> = {
  n: 'param_hint_count',
  batch_size: 'param_hint_count',
  count: 'param_hint_count',
  image_count: 'param_hint_count',
  quantity: 'param_hint_count',
  generate_count: 'param_hint_count',
  '生成数量': 'param_hint_count',
  '图片数量': 'param_hint_count',
  '生成张数': 'param_hint_count',
  prompt_enhancer: 'param_hint_prompt_enhancer',
  'prompt enhancer': 'param_hint_prompt_enhancer',
  variations: 'param_hint_variations',
  variation: 'param_hint_variations',
  resolution: 'param_hint_resolution',
  quality: 'param_hint_quality',
  layer_decomposition: 'param_hint_layer_decomposition',
};

/** 常用参数分组标题内置中文对照字典 */
export const DEFAULT_GROUP_LABELS: Record<string, string> = {
  'more options': '更多选项',
  'more settings': '更多设置',
  advanced: '高级设置',
  'advanced options': '高级选项',
};

const GROUP_LABEL_I18N_KEYS: Record<string, string> = {
  'more options': 'group_more_options',
  'more settings': 'group_more_settings',
  advanced: 'group_advanced',
  'advanced options': 'group_advanced_options',
};

/** 图片/视频方案里写死的中文口名、特性名、节点名。简体原样，其它语言走语言包。 */
const SCHEME_PHRASE_I18N: Record<string, string> = {
  '提示词': 'io_label_prompt',
  '反向提示词': 'io_label_negative',
  '参考图': 'io_label_ref_image',
  '参考视频': 'io_label_ref_video',
  '参考音频': 'io_label_ref_audio',
  '图片列表': 'io_label_images',
  '视频': 'io_label_video',
  '音频': 'io_label_audio',
  '图片': 'io_label_image',
  '图生视频': 'io_label_i2v',
  '首帧': 'io_start_frame',
  '尾帧': 'io_last_frame',
  '最后一帧': 'io_label_last_frame_out',
  '编辑视频': 'feature_edit_video',
  '延长视频': 'feature_extend',
  '深度思考': 'io_label_thinking',
  '回复': 'io_label_reply',
  '文档': 'io_label_document',
  '文件': 'io_label_file',
  '文本': 'io_label_text',
  '素材': 'io_label_asset',
  '预览': 'io_label_preview',
  '导演台': 'io_label_director',
  '火山画质增强': 'io_label_enhance',
  '参考': 'io_label_reference',
  '创建图片': 'feature_create_image',
  '编辑图片': 'feature_edit_image',
  '蒙版': 'feature_edit_mask',
  '坐标框': 'feature_edit_bbox',
  '放大图片': 'feature_upscale_image',
  '参考生视频': 'feature_reference_i2v',
  '文生视频': 'feature_text_to_video',
  '首尾帧生视频': 'feature_starting_frame',
  '理解图片': 'feature_understand_image',
  '理解音频': 'feature_understand_audio',
  '理解视频': 'feature_understand_video',
  '理解文件': 'feature_understand_file',
  '返回最后一帧': 'param_return_last_frame',
  '生成音频': 'param_generate_audio',
  '联网搜索': 'param_web_search',
  '提示词扩写': 'param_prompt_extend',
  '视频时长': 'param_duration',
  '输出分辨率': 'param_resolution',
};

const EXACT_HINT_I18N: Record<string, string> = {
  '2.0 文生/图生/全能参考 4–15 秒，2.5 为 4–30 秒。仅编辑由模型自选时长（请求传 -1）；延长视频可选手动秒数': 'hint_duration_seedance',
  '-1 为智能时长；有视频输入时输入+输出总时长≤30秒': 'hint_duration_wan',
  '文生、全能参考可选固定比例。首帧/首尾帧、编辑、延长必须 adaptive': 'hint_ratio_seedance',
  '有参考素材时建议 adaptive': 'hint_ratio_wan',
  '原生有声视频': 'hint_audio_native',
  '有声视频（台词/BGM/音效）；设为关闭对应 parameters.audio=false': 'hint_audio_wan',
  '仅文生视频生效': 'hint_web_search',
  'MiniMax-H3：4–15 秒；MiniMax-H3-Max：5–15 秒': 'hint_duration_minimax',
  '文生视频必填且不能为 adaptive；图生视频由输入图决定（adaptive，传入其他值会被忽略）；参考生可选': 'hint_ratio_minimax',
  'MiniMax-H3：768P / 2K；MiniMax-H3-Max：480P / 768P': 'hint_resolution_minimax',
  '对应官方 aigc_watermark': 'hint_watermark_minimax',
};

export function localizeSchemePhrase(text: string): string {
  const raw = (text || '').trim();
  if (!raw) return raw;
  const curLang = i18n.language || 'zh';
  if (curLang === 'zh' || curLang === 'zh-CN') return raw;
  const key = SCHEME_PHRASE_I18N[raw];
  if (!key) return raw;
  return i18n.t(`playground_2026:${key}`, raw);
}

/** 参数标签文案：优先多语言匹配，若当前为简体中文且为中文直接用，否则查字典回退 */
export const formatParamLabel = (param: SchemeParam): string => {
  const lbl = (param.label || '').trim();
  const key = (param.key || '').trim().toLowerCase();
  const lowerLbl = lbl.toLowerCase();
  const curLang = i18n.language || 'zh';
  const isSimplified = curLang === 'zh' || curLang === 'zh-CN';

  // 1. 若当前语言非简体中文，优先尝试将中文标签或键名匹配到多语言字典
  if (!isSimplified) {
    if (CHINESE_PARAM_I18N_KEYS[lbl]) {
      return i18n.t(`playground_2026:${CHINESE_PARAM_I18N_KEYS[lbl]}`, lbl);
    }
    if (PARAM_LABEL_I18N_KEYS[key]) {
      return i18n.t(`playground_2026:${PARAM_LABEL_I18N_KEYS[key]}`, DEFAULT_PARAM_LABELS[key] || lbl);
    }
    if (PARAM_LABEL_I18N_KEYS[lowerLbl]) {
      return i18n.t(`playground_2026:${PARAM_LABEL_I18N_KEYS[lowerLbl]}`, DEFAULT_PARAM_LABELS[lowerLbl] || lbl);
    }
    if (isCountParam(param)) {
      return i18n.t('playground_2026:param_count', 'Image Count');
    }
  }

  // 2. 若原文本即包含中文字符且是简体中文环境，直接采用
  if (/[\u4e00-\u9fa5]/.test(lbl)) {
    return lbl;
  }

  // 3. 英文标签/键名翻译
  if (PARAM_LABEL_I18N_KEYS[lowerLbl]) {
    return i18n.t(`playground_2026:${PARAM_LABEL_I18N_KEYS[lowerLbl]}`, DEFAULT_PARAM_LABELS[lowerLbl]);
  }
  if (PARAM_LABEL_I18N_KEYS[key]) {
    return i18n.t(`playground_2026:${PARAM_LABEL_I18N_KEYS[key]}`, DEFAULT_PARAM_LABELS[key]);
  }

  if (DEFAULT_PARAM_LABELS[lowerLbl]) {
    return DEFAULT_PARAM_LABELS[lowerLbl];
  }
  if (DEFAULT_PARAM_LABELS[key]) {
    return DEFAULT_PARAM_LABELS[key];
  }

  if (isCountParam(param)) {
    return isSimplified ? '生成数量' : i18n.t('playground_2026:param_count', 'Image Count');
  }

  return lbl || key;
};

/** 参数描述文案（描述菜单）：优先方案配置 hint/description，其次内置说明字典 */
export const formatParamHint = (param: SchemeParam): string | undefined => {
  const key = (param.key || '').trim().toLowerCase();
  const lowerLbl = (param.label || '').trim().toLowerCase();
  const curLang = i18n.language || 'zh';
  const isSimplified = curLang === 'zh' || curLang === 'zh-CN';

  // 当非简体中文时，优先使用多语言说明；方案里写死的视频说明按原文对照
  if (!isSimplified) {
    const storedHint = (param.hint || '').trim();
    if (storedHint && EXACT_HINT_I18N[storedHint]) {
      return i18n.t(`playground_2026:${EXACT_HINT_I18N[storedHint]}`, storedHint);
    }
    if (PARAM_HINT_I18N_KEYS[key]) {
      return i18n.t(`playground_2026:${PARAM_HINT_I18N_KEYS[key]}`, DEFAULT_PARAM_HINTS[key]);
    }
    if (PARAM_HINT_I18N_KEYS[lowerLbl]) {
      return i18n.t(`playground_2026:${PARAM_HINT_I18N_KEYS[lowerLbl]}`, DEFAULT_PARAM_HINTS[lowerLbl]);
    }
    if (isCountParam(param)) {
      return i18n.t('playground_2026:param_hint_count', 'Number of images generated per run');
    }
  }

  if (param.hint && param.hint.trim()) {
    return param.hint.trim();
  }
  if (param.description && param.description.trim()) {
    return param.description.trim();
  }

  if (PARAM_HINT_I18N_KEYS[key]) {
    return i18n.t(`playground_2026:${PARAM_HINT_I18N_KEYS[key]}`, DEFAULT_PARAM_HINTS[key]);
  }
  if (PARAM_HINT_I18N_KEYS[lowerLbl]) {
    return i18n.t(`playground_2026:${PARAM_HINT_I18N_KEYS[lowerLbl]}`, DEFAULT_PARAM_HINTS[lowerLbl]);
  }

  return DEFAULT_PARAM_HINTS[key] || DEFAULT_PARAM_HINTS[lowerLbl];
};
