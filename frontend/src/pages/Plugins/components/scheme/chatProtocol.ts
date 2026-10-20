/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

/** 与 backend playground_2026::chat_protocol 的 ID 保持一致。空值走内置启发式。 */

export type ChatInputProtocolId =
  | 'text_extract'
  | 'qwen_vision'
  | 'qwen_vision_file'
  | 'doubao_vision'
  | 'doubao_omni'
  | 'openai_vision';

export type ChatThinkingProfileId =
  | 'on_off'
  | 'glm52'
  | 'glm53'
  | 'deepseek'
  | 'qwen_budget'
  | 'qwen38'
  | 'doubao_high'
  | 'doubao_medium';

export type ChatProtocolOption<T extends string> = {
  id: T;
  label: string;
  detail: string;
};

export const CHAT_INPUT_PROTOCOLS: Array<ChatProtocolOption<ChatInputProtocolId>> = [
  {
    id: 'text_extract',
    label: '纯文本 · 文档抽文字',
    detail:
      '适用：GLM / DeepSeek / Qwen3.7-Max 等纯文本聊天。\n\n' +
      'messages[].content 只发字符串，或带附件时为数组：\n' +
      '  { "type": "text", "text": "用户输入 + 抽取的文档正文" }\n\n' +
      '理解文件（txt/md/PDF）：浏览器本地抽文字，拼进上述 text，不上传 file 段。\n' +
      '不发 type=file / file_url / input_file。\n\n' +
      '若同时勾选理解图片/视频/音频，仍按标准段发送（见其它协议），本预设本身不引入多模态。',
  },
  {
    id: 'qwen_vision',
    label: '千问 · 图视频 + 文档抽文字',
    detail:
      '适用：Qwen 3.5-plus / 3.5-flash / 3.6-flash。\n\n' +
      '勾选后写入 messages[].content 数组：\n' +
      '  { "type": "text", "text": "..." }\n' +
      '  { "type": "image_url", "image_url": { "url": "https://..." } }\n' +
      '  { "type": "video_url", "video_url": { "url": "https://..." } }\n\n' +
      '理解文件：抽文字进 text，不发原生 file。\n' +
      '不发 audio_url / input_audio / type=file。',
  },
  {
    id: 'qwen_vision_file',
    label: '千问 · 图视频 + 原生 file',
    detail:
      '适用：Qwen3.8-Max。\n\n' +
      '勾选后写入 messages[].content 数组：\n' +
      '  { "type": "text", "text": "..." }\n' +
      '  { "type": "image_url", "image_url": { "url": "https://..." } }\n' +
      '  { "type": "video_url", "video_url": { "url": "https://..." } }\n' +
      '  { "type": "file", "file": { "file_url": "https://..." } }\n\n' +
      '理解文件：先上传得到公网 URL，再发上面的 file 段（单条最多 1 个）。\n' +
      '不把 PDF 抽成文字，也不发 input_file / audio_url。',
  },
  {
    id: 'doubao_vision',
    label: '豆包 · 图视频 + 文档抽文字',
    detail:
      '适用：Doubao Seed 2.1 / 1.8 / Evolving / 2.0-pro 等（不含 lite/mini 音频档）。\n\n' +
      '勾选后写入 messages[].content 数组：\n' +
      '  { "type": "text", "text": "..." }\n' +
      '  { "type": "image_url", "image_url": { "url": "https://..." } }\n' +
      '  { "type": "video_url", "video_url": { "url": "https://..." } }\n\n' +
      '理解文件：抽文字进 text。方舟 Chat Completions 的原生 PDF 形态未核验，本预设故意不发 file。\n' +
      '不发 audio_url / input_audio / type=file。',
  },
  {
    id: 'doubao_omni',
    label: '豆包 · 图视频音频 + 文档抽文字',
    detail:
      '适用：doubao-seed-2-0-lite / mini（260428）。\n\n' +
      '勾选后写入 messages[].content 数组：\n' +
      '  { "type": "text", "text": "..." }\n' +
      '  { "type": "image_url", "image_url": { "url": "https://..." } }\n' +
      '  { "type": "video_url", "video_url": { "url": "https://..." } }\n' +
      '  { "type": "audio_url", "audio_url": { "url": "https://..." } }\n\n' +
      '音频保持 audio_url，不改成 input_audio。\n' +
      '理解文件：抽文字进 text，不发 type=file。',
  },
  {
    id: 'openai_vision',
    label: 'GPT · 图片 + 文档抽文字',
    detail:
      '适用：gpt-6-astra、gpt-5.6-sol / luna / terra、gpt-5.5。\n\n' +
      '勾选后写入 messages[].content 数组：\n' +
      '  { "type": "text", "text": "..." }\n' +
      '  { "type": "image_url", "image_url": { "url": "https://..." } }\n\n' +
      '理解文件：抽文字进 text，不发原生 file。\n' +
      '不发 video_url / audio_url / type=file。',
  },
];

export const CHAT_THINKING_PROFILES: Array<ChatProtocolOption<ChatThinkingProfileId>> = [
  {
    id: 'on_off',
    label: '仅开/关',
    detail:
      '适用：GLM-5.1、Doubao 1.6-flash。\n\n' +
      '开：  { "thinking": { "type": "enabled" } }\n' +
      '关：  { "thinking": { "type": "disabled" } }\n\n' +
      '不传 reasoning_effort / enable_thinking / thinking_budget。',
  },
  {
    id: 'glm52',
    label: 'GLM-5.2 及以下 高=max / 中=high',
    detail:
      '适用：GLM-5.2 及更早的 GLM。用户端档位：高 / 中 / 关，默认高。\n\n' +
      '高：  thinking.type=enabled  +  reasoning_effort="max"\n' +
      '中：  thinking.type=enabled  +  reasoning_effort="high"\n' +
      '关：  thinking.type=disabled，并去掉 reasoning_effort',
  },
  {
    id: 'glm53',
    label: 'GLM-5.3 高=xhigh / 中=high',
    detail:
      '适用：GLM-5.3。用户端档位：高 / 中 / 关，默认高。\n\n' +
      '高：  reasoning_effort="xhigh"\n' +
      '中：  reasoning_effort="high"\n' +
      '关：  reasoning_effort="none"\n\n' +
      '不传 thinking。上游不接受该字段，也不接受 max。',
  },
  {
    id: 'deepseek',
    label: 'DeepSeek 高=max / 中=high / 低=low',
    detail:
      '适用：DeepSeek-V4-Pro / Flash。档位：高 / 中 / 低 / 关，默认中。\n\n' +
      '高：  thinking.type=enabled  +  reasoning_effort="max"\n' +
      '中：  thinking.type=enabled  +  reasoning_effort="high"\n' +
      '低：  thinking.type=enabled  +  reasoning_effort="low"\n' +
      '关：  thinking.type=disabled，并去掉 reasoning_effort',
  },
  {
    id: 'qwen_budget',
    label: '千问 thinking_budget',
    detail:
      '适用：Qwen3.7-Max 等（非 3.8）。档位：高 / 中 / 低 / 关，默认高。\n\n' +
      '高：  { "enable_thinking": true }\n' +
      '中：  enable_thinking=true  +  thinking_budget=16384\n' +
      '低：  enable_thinking=true  +  thinking_budget=4096\n' +
      '关：  { "enable_thinking": false }，并去掉 budget / reasoning_effort\n\n' +
      '不传 thinking.type。',
  },
  {
    id: 'qwen38',
    label: '千问 3.8 reasoning_effort',
    detail:
      '适用：Qwen3.8-Max。档位：高 / 中 / 低 / 关，默认高。\n\n' +
      '高：  enable_thinking=true  +  reasoning_effort="xhigh"\n' +
      '中：  enable_thinking=true  +  reasoning_effort="medium"\n' +
      '低：  enable_thinking=true  +  reasoning_effort="low"\n' +
      '关：  { "enable_thinking": false }，并去掉 reasoning_effort\n\n' +
      '不传 thinking.type / thinking_budget。',
  },
  {
    id: 'doubao_high',
    label: '豆包 高中低 · 默认高',
    detail:
      '适用：Doubao Evolving 等默认「高」的豆包。档位：高 / 中 / 低 / 关。\n\n' +
      '高：  thinking.type=enabled  +  reasoning_effort="high"\n' +
      '中：  thinking.type=enabled  +  reasoning_effort="medium"\n' +
      '低：  thinking.type=enabled  +  reasoning_effort="low"\n' +
      '关：  thinking.type=disabled',
  },
  {
    id: 'doubao_medium',
    label: '豆包 高中低 · 默认中',
    detail:
      '适用：Doubao Seed 2.0 / 1.8。字段与「豆包默认高」相同，仅用户端默认档为「中」。\n\n' +
      '高/中/低/关 对应 reasoning_effort high / medium / low，或 thinking.disabled。',
  },
];

const INPUT_IDS = new Set(CHAT_INPUT_PROTOCOLS.map((p) => p.id));
const THINKING_IDS = new Set(CHAT_THINKING_PROFILES.map((p) => p.id));

export function sanitizeInputProtocol(raw?: string | null): ChatInputProtocolId | null {
  const id = (raw || '').trim();
  return INPUT_IDS.has(id as ChatInputProtocolId) ? (id as ChatInputProtocolId) : null;
}

export function sanitizeThinkingProfile(raw?: string | null): ChatThinkingProfileId | null {
  const id = (raw || '').trim();
  return THINKING_IDS.has(id as ChatThinkingProfileId) ? (id as ChatThinkingProfileId) : null;
}

/** 仅千问 3.8 预设走原生 file；其余理解文件都抽文字，避免未核验的厂商形态 400。 */
export function usesNativeFilePart(protocol?: string | null): boolean {
  return sanitizeInputProtocol(protocol) === 'qwen_vision_file';
}
