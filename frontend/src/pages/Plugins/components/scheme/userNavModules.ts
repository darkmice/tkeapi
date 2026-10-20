/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import i18n from '../../../../i18n';

export type UserNavModuleId = 'image' | 'video' | 'audio' | 'chat' | 'workflow';

export type UserNavModule = {
  id: UserNavModuleId;
  enabled: boolean;
  title: string;
};

export const USER_NAV_TITLE_MAX = 32;
const USER_NAV_FALLBACK_PATH = '/playground-2026/assets/all';

const NAV_DEFS: {
  id: UserNavModuleId;
  defaultTitle: string;
  defaultEnabled: boolean;
  path: string;
  tab: string;
  i18nKey: string;
}[] = [
  { id: 'image', defaultTitle: '图片', defaultEnabled: true, path: '/playground-2026/images', tab: 'images', i18nKey: 'playground_2026:images' },
  { id: 'video', defaultTitle: '视频', defaultEnabled: true, path: '/playground-2026/videos', tab: 'videos', i18nKey: 'playground_2026:videos' },
  { id: 'audio', defaultTitle: '音频', defaultEnabled: false, path: '/playground-2026/audios', tab: 'audios', i18nKey: 'playground_2026:audios' },
  { id: 'chat', defaultTitle: '聊天', defaultEnabled: true, path: '/playground-2026/chats', tab: 'chats', i18nKey: 'playground_2026:chats' },
  { id: 'workflow', defaultTitle: '工作流', defaultEnabled: false, path: '/playground-2026/workflows', tab: 'workflows', i18nKey: 'playground_2026:workflows' },
];

const NAV_DEF_MAP = Object.fromEntries(NAV_DEFS.map((d) => [d.id, d])) as Record<
  UserNavModuleId,
  (typeof NAV_DEFS)[number]
>;

export function defaultUserNavModules(): UserNavModule[] {
  return NAV_DEFS.map((d) => ({
    id: d.id,
    enabled: d.defaultEnabled,
    title: '',
  }));
}

export function userNavModuleDef(id: UserNavModuleId) {
  return NAV_DEF_MAP[id];
}

function sanitizeTitle(raw: unknown): string {
  if (typeof raw !== 'string') return '';
  return Array.from(raw).slice(0, USER_NAV_TITLE_MAX).join('').trim();
}

function isNavId(id: unknown): id is UserNavModuleId {
  return typeof id === 'string' && id in NAV_DEF_MAP;
}

function parseItem(raw: unknown): UserNavModule | null {
  if (!raw || typeof raw !== 'object') return null;
  const rec = raw as { id?: unknown; enabled?: unknown; title?: unknown };
  if (!isNavId(rec.id)) return null;
  const def = NAV_DEF_MAP[rec.id];
  return {
    id: rec.id,
    enabled: typeof rec.enabled === 'boolean' ? rec.enabled : def.defaultEnabled,
    title: sanitizeTitle(rec.title),
  };
}

export function normalizeUserNavModules(
  raw: unknown,
  legacy?: {
    audioEnabled?: boolean | null;
    workflowEnabled?: boolean | null;
    workflowMenuTitle?: string | null;
  },
): UserNavModule[] {
  const seen = new Set<UserNavModuleId>();
  const items: UserNavModule[] = [];
  if (Array.isArray(raw)) {
    for (const row of raw) {
      const item = parseItem(row);
      if (!item || seen.has(item.id)) continue;
      seen.add(item.id);
      items.push(item);
    }
  }
  for (const def of NAV_DEFS) {
    if (!seen.has(def.id)) {
      items.push({
        id: def.id,
        enabled: def.defaultEnabled,
        title: '',
      });
    }
  }
  if (!Array.isArray(raw) || raw.length === 0) {
    const audio = items.find((m) => m.id === 'audio');
    const workflow = items.find((m) => m.id === 'workflow');
    if (audio && typeof legacy?.audioEnabled === 'boolean') audio.enabled = legacy.audioEnabled;
    if (workflow) {
      if (typeof legacy?.workflowEnabled === 'boolean') workflow.enabled = legacy.workflowEnabled;
      if (typeof legacy?.workflowMenuTitle === 'string') {
        workflow.title = sanitizeTitle(legacy.workflowMenuTitle);
      }
    }
  }
  return items;
}

/** 后台填写的名称只替换简体中文；繁體、英语等系统语言包保持原文 */
function isSimplifiedChineseUi(): boolean {
  const raw = (i18n.resolvedLanguage || i18n.language || 'zh').trim().toLowerCase().replace('_', '-');
  if (raw === 'zh-tw' || raw.startsWith('zh-hant') || raw === 'zh-hk' || raw === 'zh-mo') return false;
  return raw === 'zh' || raw.startsWith('zh-');
}

export function resolveNavTitle(
  mod: UserNavModule,
  t: (key: string, fallback: string) => string,
): string {
  const def = NAV_DEF_MAP[mod.id];
  const trimmed = (mod.title || '').trim();
  if (trimmed && isSimplifiedChineseUi()) return trimmed;
  return t(def.i18nKey, def.defaultTitle);
}

export function enabledUserNavModules(items: UserNavModule[]): UserNavModule[] {
  return items.filter((m) => m.enabled);
}

export function firstEnabledNavPath(items: UserNavModule[]): string {
  const first = items.find((m) => m.enabled);
  return first ? NAV_DEF_MAP[first.id].path : USER_NAV_FALLBACK_PATH;
}

export function navIdFromPath(path: string): UserNavModuleId | null {
  if (path.startsWith('/playground-2026/images')) return 'image';
  if (path.startsWith('/playground-2026/videos')) return 'video';
  if (path.startsWith('/playground-2026/audios') || path.startsWith('/playground-2026/audio')) {
    return 'audio';
  }
  if (path.startsWith('/playground-2026/chats')) return 'chat';
  if (path.startsWith('/playground-2026/workflows')) return 'workflow';
  return null;
}

export function userNavFromStorageStats(storageStats: any): UserNavModule[] {
  if (!storageStats) {
    return defaultUserNavModules();
  }
  return normalizeUserNavModules(storageStats.user_nav_modules, {
    audioEnabled: storageStats.audio_enabled,
    workflowEnabled: storageStats.workflow_enabled,
    workflowMenuTitle: storageStats.workflow_menu_title,
  });
}

export function patchUserNavModule(
  items: UserNavModule[],
  id: UserNavModuleId,
  patch: Partial<Pick<UserNavModule, 'enabled' | 'title'>>,
): UserNavModule[] {
  return items.map((m) => (m.id === id ? { ...m, ...patch } : m));
}

export function moveUserNavModule(items: UserNavModule[], index: number, direction: 'up' | 'down'): UserNavModule[] {
  const target = direction === 'up' ? index - 1 : index + 1;
  if (target < 0 || target >= items.length) return items;
  const next = items.slice();
  const tmp = next[index];
  next[index] = next[target];
  next[target] = tmp;
  return next;
}
