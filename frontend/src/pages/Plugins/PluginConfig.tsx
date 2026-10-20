/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useRef, Suspense, useMemo } from 'react';
import { Typography, Switch, Button, Checkbox, Divider, Spin, Tag, Tabs, Input, InputNumber, Form, Space, Alert, Select, Table, Drawer, Radio, App, Segmented, Modal, Tooltip, Row, Col, Popconfirm } from 'antd';
import { AppMessageBridge } from '../../components/AppMessageBridge';
import { EyeOutlined, ArrowLeftOutlined, SaveOutlined, PictureOutlined, AppstoreOutlined, CloudServerOutlined, ApiOutlined, CheckCircleOutlined, LoadingOutlined, CloseCircleOutlined, SendOutlined, TeamOutlined, ExperimentOutlined, SettingOutlined, VideoCameraOutlined, PlusOutlined, DeleteOutlined, EditOutlined, CopyOutlined, ShopOutlined, MessageOutlined, ReloadOutlined, HomeOutlined, ThunderboltOutlined, InfoCircleOutlined, BookOutlined, QuestionCircleOutlined, SafetyCertificateOutlined, TagsOutlined, SearchOutlined, CheckOutlined, CloseOutlined, ArrowUpOutlined, ArrowDownOutlined, AudioOutlined, MobileOutlined, CreditCardOutlined, CustomerServiceOutlined, HistoryOutlined } from '@ant-design/icons';
import { useParams, useNavigate, useLocation, Link } from 'react-router-dom';
import request from '../../utils/request';
import { invalidateAdminPluginsCache } from '../../utils/adminPlugins';
import { invalidateActivePluginsCache } from '../../utils/activePlugins';
import { invalidateCoalesce } from '../../utils/coalesceAsync';
import { getPluginAdminTabs, resolvePluginAdminDefaultTab } from '../../utils/pluginAdminTabs';
import { modelMatchesKeyword } from '../../utils/modelKeywordMatch';
import { buildClassificationParams } from '../../utils/classificationParams';
import type { Plugin, UserLevel } from '../../types';
import {
  AdminPresetAssets,
  RelayConvertAssets,
  ApiProxyAssets,
  CloudAssetsTab,
  TeamConfig,
  ThemePromo,
  SiteIconsManager,
  PortalManager,
  PortalStyleSelection,
  PortalManagerPro,
  PortalArticleManagerPro,
  PortalStyleSelectionPro,
  PortalDocsManager,
  DocsManager,
  ApiAccessConfig,
  HaLogs,
  PlaygroundSkillConfigTab,
  PlaygroundChatConfigTab,
  PlaygroundPromptOptimizeConfigTab,
  PlaygroundErrorMappingConfigTab,
  PlaygroundImageEditConfigTab,
  PlaygroundVideoEditConfigTab,
  PlaygroundDemoImageConfig,
  PlaygroundUserNavConfig,
} from '../../plugins-registry';
import ModerationQuery from '../ModerationQuery/ModerationQuery';
import { useThemeStore } from '../../store/theme';
import ApiLogPayloadExpand from './components/ApiLogPayloadExpand';
import PluginLogRetentionCard from './components/PluginLogRetentionCard';
import SchemeIoEditor, { ensureSchemeIoDefaults } from './components/SchemeIoEditor';
import ModelIoOverridesEditor from './components/ModelIoOverridesEditor';
import { validateSchemeIoForSave } from './components/scheme/schemeIo';
import { SCHEME_QUICK_BAR_HELP, SCHEME_QUICK_BAR_MAX, countQuickBarEnabled, isQuickBarEligible } from './components/scheme/schemeQuickBar';
import { formatChoiceLabel, formatParamLabel, isCountParam, isSingleCountParam } from './components/scheme/schemeParamUtils';
import { CHAT_UNDERSTAND_ATTRS, featureKindFromTypeName, imageFeatureOptions, normalizeImageFeatureAttrs, parseFeatureAttrList, videoGenerationModesFromScheme } from './components/scheme/modelFeatures';
import { CHAT_INPUT_PROTOCOLS, CHAT_THINKING_PROFILES } from './components/scheme/chatProtocol';
import { chatProtocolSelectLabel } from './components/scheme/ChatProtocolHelpMark';
import FeatureAttributesEditor from './components/scheme/FeatureAttributesEditor';
import ImageSpecialParamsEditor from './components/scheme/ImageSpecialParamsEditor';
import SchemePromptOptimizeField from './components/scheme/SchemePromptOptimizeField';
import SchemeVoiceLibraryField from './components/scheme/SchemeVoiceLibraryField';
import {
  parsePromptOptimizeBinding,
  seedPromptOptimize,
} from './components/scheme/promptOptimize';
import {
  parseVoiceLibraryBinding,
  seedVoiceLibrary,
  type VoiceCatalogs,
} from './components/scheme/voiceLibrary';
import {
  defaultUserNavModules,
  normalizeUserNavModules,
  patchUserNavModule,
  type UserNavModule,
} from './components/scheme/userNavModules';
import {
  normalizeImageSpecialParams,
  officialImageSpecialParams,
  seedImageSpecialParams,
  isImageSpecialForcedOn,
} from './components/scheme/imageSpecialParams';
import useSettingsStore from '../../store/settings';
import { StorageConfigPanel, getStorageProvider } from '../../components/Storage';
import ClassificationFilter from '../../components/Models/ClassificationFilter';
import { listPagination, useListPager } from '../../components/ListPagination';
import MarketplaceTrendingTab from './ModelMarketplace/MarketplaceTrendingTab';
import { useTranslation } from 'react-i18next';
import { formatApiDateTime } from '../../utils/timedisplay';
import {
  HA_DEFAULT_MELT,
  HA_MELT_CODES,
  haMeltLabel,
  haMeltSummary,
} from '../../constants/relayStatusCodes';

// ── 物理级完全解耦动态插件扫描 ──
const dynamicMeta = import.meta.glob('./**/plugin_meta.ts', { eager: true });
const dynamicPlugins: Record<
  string,
  {
    name: string;
    title: string;
    component?: () => Promise<any>;
    tabs?: { key: string; label: string; component: () => Promise<any> }[];
  }
> = {};

Object.entries(dynamicMeta).forEach(([path, module]: [string, any]) => {
  const meta = module.default;
  if (meta && meta.name) {
    dynamicPlugins[meta.name] = meta;
  }
});

// ── 插件组件动态加载（各插件均可独立移除，删除对应目录后自动降级为「该插件模块暂未安装」） ──
const safeLazy = (loader: () => Promise<any>) =>
  React.lazy(() =>
    loader().catch(() => ({ default: () => <div style={{ padding: 40, textAlign: 'center', color: '#999' }}>该插件模块暂未安装</div> }))
  );

/** 按 tab key 缓存 lazy，避免父组件重渲染时换类型导致误卸载 */
const lazyByKey = new Map<string, React.LazyExoticComponent<React.ComponentType<any>>>();
const cachedLazy = (key: string, loader: () => Promise<any>) => {
  let Comp = lazyByKey.get(key);
  if (!Comp) {
    Comp = safeLazy(loader);
    lazyByKey.set(key, Comp);
  }
  return Comp;
};

/** 插件组件包装器：Suspense + 降级 */
const PluginModule: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <Suspense fallback={<div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', minHeight: 400, height: '100%' }}><Spin size="large" /></div>}>{children}</Suspense>
);

/** 使用独立 TOS 表单的插件（其余仅展示全局存储） */
const INDEPENDENT_STORAGE_PLUGINS = new Set([
  'asset_manager',
  'asset_manager_intl',
  'playground',
  'playground_2026',
  'upstream_asset_relay',
]);

const SCHEME_CONTROL_TYPE_OPTIONS = [
  { label: 'Input 文本框', value: 'input' },
  { label: 'Radio 单选分段', value: 'radio' },
  { label: 'Select 下拉', value: 'select' },
  { label: 'Switch 开关', value: 'switch' },
  { label: 'Slider 滑块', value: 'slider' },
];

function sameJsonValue(a: any, b: any) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** 把当前方案参数改写成逼近目标方案（系统默认）的模型级覆写 */
function buildParamOverridesToward(baseParams: any[] | undefined, targetParams: any[] | undefined) {
  const base = Array.isArray(baseParams) ? baseParams : [];
  const target = Array.isArray(targetParams) ? targetParams : [];
  const baseByKey = new Map(base.filter((p) => p?.key).map((p) => [p.key, p]));
  const targetKeys = new Set(target.map((p) => p?.key).filter(Boolean));
  const remove = base.map((p) => p.key).filter((k) => k && !targetKeys.has(k));
  const add: any[] = [];
  const modify: Record<string, any> = {};
  for (const tp of target) {
    if (!tp?.key) continue;
    const bp = baseByKey.get(tp.key);
    if (!bp) {
      add.push(JSON.parse(JSON.stringify(tp)));
      continue;
    }
    const patch: Record<string, any> = {};
    const fields = new Set([...Object.keys(bp), ...Object.keys(tp)]);
    for (const field of fields) {
      if (field === 'key') continue;
      if (!sameJsonValue(bp[field], tp[field])) patch[field] = tp[field];
    }
    if (Object.keys(patch).length) modify[tp.key] = patch;
  }
  const out: { modify?: Record<string, any>; remove?: string[]; add?: any[] } = {};
  if (Object.keys(modify).length) out.modify = modify;
  if (remove.length) out.remove = remove;
  if (add.length) out.add = add;
  return Object.keys(out).length ? out : null;
}

function isSeedream50ProModel(model?: { model_id?: string; name?: string; mid?: string } | null) {
  const hay = `${model?.model_id || ''} ${model?.name || ''} ${model?.mid || ''}`
    .toLowerCase()
    .replace(/[_\s.]+/g, '-');
  return hay.includes('seedream-5-0-pro') || hay.includes('seedream-5-pro');
}

function asFiniteNumber(v: unknown): number | undefined {
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  if (typeof v === 'string' && v.trim() !== '') {
    const n = Number(v);
    if (Number.isFinite(n)) return n;
  }
  return undefined;
}

function numericParamOptions(options: unknown): number[] {
  if (!Array.isArray(options)) return [];
  return options
    .map(asFiniteNumber)
    .filter((n): n is number => n !== undefined);
}

function optionsFromSliderRange(min: number, max: number, step: number): number[] {
  const s = step > 0 ? step : 1;
  if (!Number.isFinite(min) || !Number.isFinite(max) || max < min) return [];
  const count = Math.floor((max - min) / s) + 1;
  if (count <= 0) return [];
  if (count > 64) return [min, max];
  const out: number[] = [];
  for (let i = 0; i < count; i += 1) {
    out.push(Number((min + i * s).toFixed(8)));
  }
  return out;
}

/** 调参面板切换控件类型时，补齐该类型需要的字段 */
function patchForControlTypeChange(base: any, currentMod: any, nextType: string): Record<string, any> {
  const merged = { ...base, ...currentMod };
  const patch: Record<string, any> = { type: nextType };
  const nums = numericParamOptions(merged.options);
  if (nextType === 'slider') {
    patch.data_type = merged.data_type === 'number' ? 'number' : 'integer';
    if (nums.length) {
      patch.min = Math.min(...nums);
      patch.max = Math.max(...nums);
    } else {
      patch.min = asFiniteNumber(merged.min) ?? 0;
      patch.max = asFiniteNumber(merged.max) ?? 100;
    }
    if (patch.max < patch.min) patch.max = patch.min;
    patch.step = asFiniteNumber(merged.step) ?? 1;
    patch.options = [];
    if (typeof merged.default === 'boolean' || merged.default === undefined || merged.default === '') {
      patch.default = patch.min;
    }
  } else if (nextType === 'select' || nextType === 'radio') {
    if (!Array.isArray(merged.options) || merged.options.length === 0) {
      const min = asFiniteNumber(merged.min) ?? 1;
      const max = asFiniteNumber(merged.max) ?? min;
      const step = asFiniteNumber(merged.step) ?? 1;
      const generated = optionsFromSliderRange(min, max, step);
      if (generated.length) patch.options = generated;
    }
    if (merged.data_type === 'boolean') {
      patch.data_type = nums.length ? 'integer' : 'string';
    }
  } else if (nextType === 'switch') {
    patch.data_type = 'boolean';
    patch.default = !!merged.default;
  } else if (nextType === 'input' && merged.data_type === 'boolean') {
    patch.data_type = 'string';
  }
  const nextMerged = { ...merged, ...patch };
  if (nextMerged.quick && !isQuickBarEligible(nextMerged)) {
    patch.quick = false;
  }
  return patch;
}

const { Title, Text } = Typography;

interface StorageConfig {
  default_provider?: string;
  tos_access_key: string;
  tos_secret_key_masked: string;
  tos_endpoint: string;
  tos_region: string;
  tos_bucket: string;
  tos_path_prefix: string;
  tos_custom_domain: string;
  cos_secret_id?: string;
  cos_secret_key_masked?: string;
  cos_endpoint?: string;
  cos_region?: string;
  cos_bucket?: string;
  cos_path_prefix?: string;
  cos_custom_domain?: string;
  is_configured: boolean;
  global_configured?: boolean;
  global_provider?: string;
  global_bucket?: string;
  global_endpoint?: string;
  global_region?: string;
  global_path_prefix?: string;
}

interface ModerationConfig {
  volc_access_key: string;
  volc_secret_key_masked: string;
  volc_app_id: string;
  volc_project_name: string;
  volc_group_id: string;
  volc_region: string;
  review_api_url: string;
  is_configured: boolean;
  review_enabled: boolean;
}



type HaRuleDto = {
  id: string;
  name: string;
  retries: number;
  budget: number;
  ttfb?: number;
  err: 'first' | 'last';
  melt: Record<string, number>;
  allow?: string[];
  deny?: string[];
};

const HA_MAX_RETRIES = 100;
const HA_MAX_RULES = 32;

function newHaRuleId(): string {
  return `r${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

function newHaRule(): HaRuleDto {
  return {
    id: newHaRuleId(),
    name: '新规则',
    retries: 3,
    budget: 900,
    ttfb: 0,
    err: 'first',
    melt: { ...HA_DEFAULT_MELT },
    allow: [],
    deny: [],
  };
}

// ── 插件图标映射（各插件均可独立移除，未匹配时使用默认图标 AppstoreOutlined） ──
const pluginIcons: Record<string, React.ReactNode> = {
  asset_manager: <PictureOutlined style={{ fontSize: 20 }} />,
  asset_manager_intl: <PictureOutlined style={{ fontSize: 20 }} />,
  team_marketing: <TeamOutlined style={{ fontSize: 20 }} />,
  playground: <ExperimentOutlined style={{ fontSize: 20 }} />,
  playground_2026: <ExperimentOutlined style={{ fontSize: 20 }} />,

  model_marketplace: <ShopOutlined style={{ fontSize: 20 }} />,
  site_icons: <AppstoreOutlined style={{ fontSize: 20 }} />,

  site_portal: <HomeOutlined style={{ fontSize: 20 }} />,
  site_portal_pro: <HomeOutlined style={{ fontSize: 20 }} />,
  docs_api: <BookOutlined style={{ fontSize: 20 }} />,
  comfyui_bridge: <VideoCameraOutlined style={{ fontSize: 20 }} />,
  content_security: <SafetyCertificateOutlined style={{ fontSize: 20 }} />,
  mobile_app: <MobileOutlined style={{ fontSize: 20 }} />,
  marketing_membership: <CreditCardOutlined style={{ fontSize: 20 }} />,
  support_tickets: <CustomerServiceOutlined style={{ fontSize: 20 }} />,
  logs_archive: <HistoryOutlined style={{ fontSize: 20 }} />,
};

/** 与后端 `MP_GUEST_LEVEL_ID` 一致：未登录游客，不是真实 user_levels 行。 */
const MP_GUEST_LEVEL_ID = -1;
const MP_GUEST_LEVEL_NAME = '未登录游客';

const normalizeMpLevelId = (v: number | string): number | string =>
  typeof v === 'string' && /^-?\d+$/.test(v) && Number.isSafeInteger(Number(v)) ? Number(v) : v;

const MpLevelSelectCell: React.FC<{
  record: any;
  levels: UserLevel[];
  _isLight: boolean;
  showInactiveWarning?: boolean;
  onToggle: (id: number | string, enabled: boolean) => void;
  onChange: (id: number | string, vals: (number | string)[]) => void;
}> = React.memo(({ record, levels, _isLight, showInactiveWarning, onToggle, onChange }) => {
  const { t: tr } = useTranslation('playground_2026');
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const isMpEnabled = !!record.mp_enabled;
  const isModelDisabled = showInactiveWarning && Number(record.is_active) !== 1;
  const guestLevelName = tr('admin_mp_guest_level', MP_GUEST_LEVEL_NAME);

  const selectedLevels = useMemo(() => {
    const ids: (number | string)[] = record.mp_level_ids || [];
    return ids.map(id => {
      if (String(id) === String(MP_GUEST_LEVEL_ID)) {
        return { id, name: guestLevelName };
      }
      const found = levels.find(l => String(l.id) === String(id));
      return { id, name: found ? found.name : `ID: ${id}` };
    });
  }, [record.mp_level_ids, levels, guestLevelName]);

  const tooltipTitle = selectedLevels.length > 0 ? (
    <div style={{ maxWidth: 300, padding: '2px 0' }}>
      <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 6, color: 'rgba(255,255,255,0.95)' }}>
        {isMpEnabled
          ? tr('admin_mp_unviewable_levels', { count: selectedLevels.length, defaultValue: `不可查看的用户等级 (共 ${selectedLevels.length} 项)：` })
          : tr('admin_mp_viewable_levels', { count: selectedLevels.length, defaultValue: `可查看的用户等级 (共 ${selectedLevels.length} 项)：` })}
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, maxHeight: 180, overflowY: 'auto' }}>
        {selectedLevels.map(lv => (
          <Tag
            key={lv.id}
            color="processing"
            style={{
              margin: 0,
              fontSize: 11,
              lineHeight: '20px',
              padding: '0 6px',
              borderRadius: 4,
            }}
          >
            {lv.name}
          </Tag>
        ))}
      </div>
    </div>
  ) : null;

  return (
    <Tooltip
      title={tooltipTitle}
      placement="topLeft"
      {...(isDropdownOpen ? { open: false } : {})}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 6 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <Tooltip title={isModelDisabled ? tr('admin_mp_model_disabled_tip', '模型已被禁用，即使开启也不会在广场中显示') : ''}>
              <Switch
                size="small"
                checked={isMpEnabled}
                onChange={(val) => onToggle(record.id, val)}
                style={isModelDisabled ? { opacity: 0.5 } : {}}
              />
            </Tooltip>
            <span
              style={{
                fontSize: 11,
                fontWeight: 500,
                color: isMpEnabled
                  ? (_isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)')
                  : (_isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)'),
              }}
            >
              {isMpEnabled ? tr('admin_mp_on_unviewable', '开启 · 不可查看等级') : tr('admin_mp_off_viewable', '关闭 · 可查看等级')}
            </span>
          </div>
          {selectedLevels.length > 0 && (
            <span style={{ color: _isLight ? '#1677ff' : '#4096ff', fontSize: 11, fontWeight: 500, whiteSpace: 'nowrap' }}>
              {tr('admin_mp_selected_count', { count: selectedLevels.length, defaultValue: `已选 ${selectedLevels.length} 项` })}
            </span>
          )}
        </div>
        <Select
          mode="multiple"
          size="small"
          allowClear
          showSearch
          optionFilterProp="label"
          maxTagCount="responsive"
          maxTagPlaceholder={(omittedValues) => `+${omittedValues.length}...`}
          placeholder={isMpEnabled ? tr('admin_mp_ph_on', '默认全部可查看（可选不可看等级）') : tr('admin_mp_ph_off', '默认全部不可查看（可选可看等级）')}
          value={record.mp_level_ids || []}
          onOpenChange={setIsDropdownOpen}
          onChange={(vals: (number | string)[]) => onChange(record.id, vals.map(normalizeMpLevelId))}
          options={[
            { label: guestLevelName, value: MP_GUEST_LEVEL_ID },
            ...levels.map(lv => ({ label: lv.name, value: lv.id })),
          ]}
          style={{ width: '100%' }}
        />
      </div>
    </Tooltip>
  );
});

const PluginConfigInner: React.FC = () => {
  const { t } = useTranslation(['translation', 'playground_2026']);
  const { themeMode } = useThemeStore();
  const _isLight = themeMode === 'light';
  const { message } = App.useApp();
  const { name } = useParams<{ name: string }>();
  const isPg2026 = name === 'playground_2026';
  const pg2026T = (k: string, fb: string, opt?: Record<string, any>) =>
    isPg2026 ? t(`playground_2026:${k}`, { defaultValue: fb, ...opt }) : fb;
  const renderPgOptionPreview = (param: any) => {
    if (!isPg2026 || !Array.isArray(param?.options) || param.options.length === 0) return null;
    const labels = param.options.map((opt: string | number) => formatChoiceLabel(param, opt)).join(' / ');
    const key = String(param?.key || '').toLowerCase();
    const builtin = key === 'background' || key === 'optimize_prompt_mode';
    const hintColor = _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)';
    return (
      <>
        <Text style={{ fontSize: 11, display: 'block', marginTop: 4, color: hintColor }}>
          {pg2026T('admin_option_preview', '用户端显示：{{labels}}', { labels })}
        </Text>
        {builtin ? (
          <Text style={{ fontSize: 11, display: 'block', marginTop: 2, color: hintColor }}>
            {pg2026T('admin_builtin_option_hint', '内置文案（不透明、透明、标准、快速）会随界面语言切换。写成其它文字则固定显示。')}
          </Text>
        ) : null}
      </>
    );
  };
  const navigate = useNavigate();
  const location = useLocation();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const [plugin, setPlugin] = useState<Plugin | null>(null);
  const [levels, setLevels] = useState<UserLevel[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [isAllLevels, setIsAllLevels] = useState(true);
  const [selectedLevels, setSelectedLevels] = useState<string[]>([]);
  const [levelQuotas, setLevelQuotas] = useState<Record<string, number>>({});
  const [defaultQuota, setDefaultQuota] = useState<number>(100);
  const [levelMaxFolders, setLevelMaxFolders] = useState<Record<string, number>>({});
  const [defaultMaxFolders, setDefaultMaxFolders] = useState<number>(20);
  const [levelMaxFilesPerFolder, setLevelMaxFilesPerFolder] = useState<Record<string, number>>({});
  const [defaultMaxFilesPerFolder, setDefaultMaxFilesPerFolder] = useState<number>(100);
  const [levelMaxProjects, setLevelMaxProjects] = useState<Record<string, number>>({});
  const [defaultMaxProjects, setDefaultMaxProjects] = useState<number>(3);
  const [levelMaxAssets, setLevelMaxAssets] = useState<Record<string, number>>({});
  const [defaultMaxAssets, setDefaultMaxAssets] = useState<number>(10);
  /** 创作中心2026：单个工作流节点上限 */
  const [workflowNodeLimit, setWorkflowNodeLimit] = useState<number>(200);
  /** 创作中心2026：用户端左侧功能菜单（顺序即展示顺序） */
  const [userNavModules, setUserNavModules] = useState<UserNavModule[]>(() => defaultUserNavModules());
  const workflowNav = userNavModules.find((m) => m.id === 'workflow');
  const workflowEnabled = !!workflowNav?.enabled;
  const workflowMenuTitle = workflowNav?.title || '';
  /** 创作中心2026：登录后进入创作中心主页 */
  const [siteDefaultEnabled, setSiteDefaultEnabled] = useState(false);
  const [showInPlaygroundPrompt, setShowInPlaygroundPrompt] = useState<boolean>(false);
  const [docsApiAllowGuest, setDocsApiAllowGuest] = useState<boolean>(false);
  const [showInAdminMenu, setShowInAdminMenu] = useState(false);
  const [adminMenuSort, setAdminMenuSort] = useState(0);
  const [adminMenuTitle, setAdminMenuTitle] = useState('');
  const [adminMenuDefaultTab, setAdminMenuDefaultTab] = useState('');

  // 管理员等级（系统增强插件使用）
  const [adminGroups, setAdminGroups] = useState<{ id: number; name: string; description?: string }[]>([]);
  const [selectedAdminGroups, setSelectedAdminGroups] = useState<number[]>([]);
  const [isAllAdminGroups, setIsAllAdminGroups] = useState(true);

  // 存储配置
  const [storageConfig, setStorageConfig] = useState<StorageConfig | null>(null);
  const [storageForm] = Form.useForm();
  const [haDef, setHaDef] = useState('r1');
  const [haRules, setHaRules] = useState<HaRuleDto[]>([]);
  const [haDraft, setHaDraft] = useState<HaRuleDto | null>(null);
  const [haDraftIsNew, setHaDraftIsNew] = useState(false);
  const [haCopySource, setHaCopySource] = useState<HaRuleDto | null>(null);
  const [haCopyName, setHaCopyName] = useState('');
  const [savingStorage, setSavingStorage] = useState(false);
  const [activeTabKey, setActiveTabKey] = useState(() => {
    const hash = window.location.hash.replace('#', '');
    const tabs = getPluginAdminTabs(name || '', name ? dynamicPlugins[name] : undefined, (k, fb) => t(k, { defaultValue: fb }));
    if (tabs.some((t) => t.key === hash)) return hash;
    return tabs[0]?.key ?? 'basic';
  });

  useEffect(() => {
    if (!name) return;
    const hash = location.hash.replace('#', '');
    const tabs = getPluginAdminTabs(name, dynamicPlugins[name], (k, fb) => t(k, { defaultValue: fb }));
    if (tabs.some((t) => t.key === hash)) {
      setActiveTabKey(hash);
      return;
    }
    setActiveTabKey(tabs[0]?.key ?? 'basic');
  }, [name, location.hash, t]);

  // 栏目文章重置计数器（点击或切换到「栏目文章」Tab 时，重置到默认初始页面：栏目分类管理）
  const [articleTabResetKey, setArticleTabResetKey] = useState(0);

  const handleTabClick = (key: string) => {
    if (key === 'article_manager') {
      setArticleTabResetKey(prev => prev + 1);
    }
  };

  const handleTabChange = (key: string) => {
    setActiveTabKey(key);
    window.location.hash = key;
    if (key === 'article_manager') {
      setArticleTabResetKey(prev => prev + 1);
    }
  };

  // 审核配置
  const [moderationConfig, setModerationConfig] = useState<ModerationConfig | null>(null);
  const [moderationForm] = Form.useForm();
  const [savingModeration, setSavingModeration] = useState(false);

  // 审核日志展开详情
  const [expandedAssetInfo, setExpandedAssetInfo] = useState<Record<string, any>>({});
  const [loadingAssetInfo, setLoadingAssetInfo] = useState<Record<string, boolean>>({});


  // 接口日志
  const [apiLogs, setApiLogs] = useState<any[]>([]);
  const [apiLogsTotal, setApiLogsTotal] = useState(0);
  const { page: apiLogsPage, pageSize: apiLogsPageSize, setPage: setApiLogsPage, setPageSize: setApiLogsPageSize } = useListPager();
  const [apiLogsLoading, setApiLogsLoading] = useState(false);
  const [logSourceFilter, setLogSourceFilter] = useState<string>('');
  const [logKeyword, setLogKeyword] = useState<string>('');
  const [logStatusFilter, setLogStatusFilter] = useState<string>('');

  // ====== 模型创作中心 (Playground) 配置 Tab ======
  const [pgModels, setPgModels] = useState<any[]>([]);
  const { page: pgPage, pageSize: pgPageSize, onChange: onPgPageChange } = useListPager();
  const [pgAdvancedNodesEnabled, setPgAdvancedNodesEnabled] = useState<boolean>(false);
  const [pgAdvancedNodePreviewEnabled, setPgAdvancedNodePreviewEnabled] = useState<boolean>(true);
  const [pgAdvancedNodeVolcEnhanceEnabled, setPgAdvancedNodeVolcEnhanceEnabled] = useState<boolean>(false);
  const [pgAdvancedNodeDirectorEnabled, setPgAdvancedNodeDirectorEnabled] = useState<boolean>(false);
  const [pgAdvancedNodePromptEnabled, setPgAdvancedNodePromptEnabled] = useState<boolean>(true);
  const [pgAdvancedNodeAiVideoEnabled, setPgAdvancedNodeAiVideoEnabled] = useState<boolean>(true);
  const [pgAdvancedNodeAiImageEnabled, setPgAdvancedNodeAiImageEnabled] = useState<boolean>(true);
  const [pgAdvancedNodeAgentEnabled, setPgAdvancedNodeAgentEnabled] = useState<boolean>(false);
  const [pgAdvancedNodesUnifiedLimitEnabled, setPgAdvancedNodesUnifiedLimitEnabled] = useState<boolean>(false);
  const [pgAdvancedNodesUnifiedLimitValue, setPgAdvancedNodesUnifiedLimitValue] = useState<number>(10);
  const [pgAdvancedNodePreviewLimit, setPgAdvancedNodePreviewLimit] = useState<number>(10);
  const [pgAdvancedNodePromptLimit, setPgAdvancedNodePromptLimit] = useState<number>(10);
  const [pgAdvancedNodeAiVideoLimit, setPgAdvancedNodeAiVideoLimit] = useState<number>(10);
  const [pgAdvancedNodeAiImageLimit, setPgAdvancedNodeAiImageLimit] = useState<number>(10);
  const [pgAdvancedNodeAgentLimit, setPgAdvancedNodeAgentLimit] = useState<number>(10);
  const [pgAdvancedNodeVolcEnhanceLimit, setPgAdvancedNodeVolcEnhanceLimit] = useState<number>(10);
  const [pgAdvancedNodeInstanceLimit, setPgAdvancedNodeInstanceLimit] = useState<number>(50);
  const [pgAgentModeEnabled, setPgAgentModeEnabled] = useState<boolean>(false);
  const [pgAgentVideoMode, setPgAgentVideoMode] = useState<string>('track');
  const [pgAgentWelcomeTitle, setPgAgentWelcomeTitle] = useState<string>('');
  const [pgAgentWelcomeDesc, setPgAgentWelcomeDesc] = useState<string>('');
  const [pgAgentPresetPrompts, setPgAgentPresetPrompts] = useState<any[]>([]);
  const [pgAgentSystemPrompt, setPgAgentSystemPrompt] = useState<string>('');
  const [pgAgentChatModels, setPgAgentChatModels] = useState<string[]>([]);
  const [volcEnhancePluginActive, setVolcEnhancePluginActive] = useState<boolean>(false);
  const [pgSchemes, setPgSchemes] = useState<any[]>([]);
  const [savingPlayground, setSavingPlayground] = useState(false);
  const [savingWorkflowConfig, setSavingWorkflowConfig] = useState(false);
  const [pgSearchKeyword, setPgSearchKeyword] = useState('');
  const [editingSortModelId, setEditingSortModelId] = useState<number | null>(null);
  const [editingSortValue, setEditingSortValue] = useState<number>(0);
  const [pgEnabledFilter, setPgEnabledFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [pgSchemeTypeFilter, setPgSchemeTypeFilter] = useState('all');
  const [pgSchemeDrawerVisible, setPgSchemeDrawerVisible] = useState(false);
  const [pgCurrentId, setPgCurrentId] = useState<number | null>(null);
  const [pgSelectedSchemeId, setPgSelectedSchemeId] = useState<string>('');
  const [pgDefaultModelMids, setPgDefaultModelMids] = useState<string[]>([]);
  // 分类过滤
  const [pgProvidersStats, setPgProvidersStats] = useState<any[]>([]);
  const [pgApiProvidersStats, setPgApiProvidersStats] = useState<any[]>([]);
  const [pgTypesStats, setPgTypesStats] = useState<any[]>([]);
  const [pgSelectedProvider, setPgSelectedProvider] = useState<number | null>(null);
  const [pgSelectedApiProvider, setPgSelectedApiProvider] = useState<number | null>(null);
  const [pgSelectedType, setPgSelectedType] = useState<number | null>(null);
  const [pgModelTypes, setPgModelTypes] = useState<{ id: number; name: string; default_features: string[] }[]>([]);
  const [pgFeatureModalVisible, setPgFeatureModalVisible] = useState(false);
  const [pgFeatureModelId, setPgFeatureModelId] = useState<number | null>(null);
  const [pgFeatureDraft, setPgFeatureDraft] = useState<string[]>([]);
  const [pgInputProtocolDraft, setPgInputProtocolDraft] = useState<string>('');
  const [pgThinkingProfileDraft, setPgThinkingProfileDraft] = useState<string>('');
  const [pgFeatureSaving, setPgFeatureSaving] = useState(false);
  // 参数覆写 Modal
  const [pgOverrideModalVisible, setPgOverrideModalVisible] = useState(false);
  const [pgOverrideModelId, setPgOverrideModelId] = useState<number | null>(null);
  const [pgOverrideData, setPgOverrideData] = useState<any>({ modify: {}, remove: [], add: [] });
  // IO 覆写 Modal
  const [pgIoOverrideModalVisible, setPgIoOverrideModalVisible] = useState(false);
  const [pgIoOverrideModelId, setPgIoOverrideModelId] = useState<number | null>(null);
  const [pgIoOverrideData, setPgIoOverrideData] = useState<any>(null);
  const [pgIspModalVisible, setPgIspModalVisible] = useState(false);
  const [pgIspDraft, setPgIspDraft] = useState<any>(null);
  const [pgIspBaseline, setPgIspBaseline] = useState<any>(null);
  const [pgPoDraft, setPgPoDraft] = useState<any>(null);
  const [pgVlDraft, setPgVlDraft] = useState<any>(null);
  const [pgVoiceCatalogs, setPgVoiceCatalogs] = useState<VoiceCatalogs>({});

  // ====== 模型广场管理 (Model Marketplace) 配置 ======
  const [mpModels, setMpModels] = useState<any[]>([]);
  const { page: mpPage, pageSize: mpPageSize, onChange: onMpPageChange } = useListPager();
  const [savingMarketplace, setSavingMarketplace] = useState(false);
  const [mpSearchKeyword, setMpSearchKeyword] = useState('');
  const [mpProviderFilter, setMpProviderFilter] = useState<string>('all');
  const [mpTypeFilter, setMpTypeFilter] = useState<string>('all');
  const [mpStatusFilter, setMpStatusFilter] = useState<string>('all'); // 'all' | 'enabled' | 'disabled'
  const [mpModelActiveFilter, setMpModelActiveFilter] = useState<string>('all'); // 'all' | 'active' | 'inactive' (模型管理里的状态)
  const [mpDisplayMode, setMpDisplayMode] = useState<'whitelist' | 'blacklist'>('whitelist');
  const [mpAllowGuest, setMpAllowGuest] = useState<boolean>(false);
  const [mpTrendingConfig, setMpTrendingConfig] = useState<any>({ enabled: false, hero: {}, sections: [] });

  const fetchMarketplaceConfig = async () => {
    try {
      const res = await (request.get(`/plugins/${name}/marketplace-models`) as Promise<any>);
      if (res.models) {
        const sorted = [...res.models].sort((a, b) => (Number(b.mp_sort_order) || 0) - (Number(a.mp_sort_order) || 0));
        setMpModels(sorted);
      }
      if (res.display_mode) setMpDisplayMode(res.display_mode);
      if (res.allow_guest !== undefined) {
        setMpAllowGuest(res.allow_guest);
        setDocsApiAllowGuest(res.allow_guest);
      }
      if (res.trending_config) {
        setMpTrendingConfig(res.trending_config);
      }
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    if (name === 'model_marketplace') {
      fetchMarketplaceConfig();
    }
  }, [name]);

  const modelSortTimers = useRef<Record<string, ReturnType<typeof setTimeout>>>({});

  const persistModelPageSort = async (id: number | string, sort: number) => {
    await request.put(`/models/${id}`, { sort_order: sort });
  };

  const persistModelPageSortDebounced = (id: number | string, sort: number) => {
    const key = String(id);
    const prev = modelSortTimers.current[key];
    if (prev) clearTimeout(prev);
    modelSortTimers.current[key] = setTimeout(() => {
      persistModelPageSort(id, sort).catch((e) => {
        console.error(e);
        message.error('排序保存失败');
      });
    }, 300);
  };

  const flushPendingModelPageSorts = async () => {
    const pendingIds = Object.keys(modelSortTimers.current);
    for (const timer of Object.values(modelSortTimers.current)) {
      clearTimeout(timer);
    }
    modelSortTimers.current = {};
    await Promise.all(pendingIds.map((id) => {
      const row = mpModels.find((x) => String(x.id) === String(id));
      if (!row) return Promise.resolve();
      return persistModelPageSort(id, Number(row.mp_sort_order) || 0);
    }));
  };

  const handleSaveMarketplaceConfig = async () => {
    try {
      setSavingMarketplace(true);
      await flushPendingModelPageSorts();
      const payload = {
        display_mode: mpDisplayMode,
        allow_guest: mpAllowGuest,
        trending_config: mpTrendingConfig,
        models: mpModels.map(m => ({
          id: typeof m.id === 'string' && /^\d+$/.test(m.id) && Number.isSafeInteger(Number(m.id)) ? Number(m.id) : m.id,
          enabled: !!m.mp_enabled,
          sort_order: Number(m.mp_sort_order) || 0,
          description: m.mp_description || '',
          description_en: m.mp_description_en || '',
          level_ids: (m.mp_level_ids || []).map((lv: any) => normalizeMpLevelId(lv))
        }))
      };
      await request.post(`/plugins/${name}/marketplace-models`, payload);
      invalidateCoalesce('pg2026:public-config-and-user-configs');
      message.success('模型广场配置保存成功');
    } catch (e) {
      console.error(e);
    } finally {
      setSavingMarketplace(false);
    }
  };

  const handleMpToggle = async (id: number | string, enabled: boolean) => {
    const target = mpModels.find(m => String(m.id) === String(id)) || pgModels.find(m => String(m.id) === String(id));
    const prevLevelIds = target?.mp_level_ids || [];
    setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_enabled: enabled, mp_level_ids: [] } : m));
    setPgModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_enabled: enabled, mp_level_ids: [] } : m));
    try {
      await request.post('/plugins/model_marketplace/marketplace-models/toggle', {
        id,
        enabled,
        sort_order: Number(target?.mp_sort_order ?? target?.pg_sort_order) || 0,
        description: target?.mp_description,
        description_en: target?.mp_description_en,
      });
      invalidateCoalesce('pg2026:public-config-and-user-configs');
      message.success(enabled ? '已开启广场展示' : '已关闭广场展示');
    } catch (e) {
      console.error(e);
      message.error('切换广场展示状态失败');
      setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_enabled: !enabled, mp_level_ids: prevLevelIds } : m));
      setPgModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_enabled: !enabled, mp_level_ids: prevLevelIds } : m));
    }
  };

  const handleMpLevelIdsChange = async (id: number | string, levelIds: (number | string)[]) => {
    const normalized = (levelIds || []).map((lv: any) => normalizeMpLevelId(lv));
    const target = mpModels.find(m => String(m.id) === String(id)) || pgModels.find(m => String(m.id) === String(id));
    const prevLevelIds = target?.mp_level_ids || [];
    setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_level_ids: normalized } : m));
    setPgModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_level_ids: normalized } : m));
    try {
      await request.post('/plugins/model_marketplace/marketplace-models/toggle', {
        id,
        enabled: !!target?.mp_enabled,
        level_ids: normalized,
        sort_order: Number(target?.mp_sort_order ?? target?.pg_sort_order) || 0,
        description: target?.mp_description,
        description_en: target?.mp_description_en,
      });
      invalidateCoalesce('pg2026:public-config-and-user-configs');
    } catch (e) {
      console.error(e);
      message.error('更新用户等级失败');
      setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_level_ids: prevLevelIds } : m));
      setPgModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_level_ids: prevLevelIds } : m));
    }
  };

  const handleMpSortChange = (id: number | string, sort: number) => {
    setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, mp_sort_order: sort } : m));
    persistModelPageSortDebounced(id, sort);
  };

  const handleMpDescChange = (id: number | string, desc: string, language: 'zh' | 'en' = 'zh') => {
    const field = language === 'en' ? 'mp_description_en' : 'mp_description';
    setMpModels(prev => prev.map(m => String(m.id) === String(id) ? { ...m, [field]: desc } : m));
  };

  const fetchPlaygroundConfigBase = async () => {
    try {
      const res = await (request.get(`/plugins/${name}/playground-config`) as Promise<any>);
      if (res.models) {
        const sorted = [...res.models].sort((a, b) => (b.pg_sort_order || 0) - (a.pg_sort_order || 0));
        setPgModels(sorted);
      }
      if (res.schemes) setPgSchemes(res.schemes);
      if (res.voice_catalogs && typeof res.voice_catalogs === 'object') {
        setPgVoiceCatalogs(res.voice_catalogs);
      }
      if (Array.isArray(res.model_types)) {
        setPgModelTypes(res.model_types.map((t: any) => ({
          id: t.id,
          name: t.name || '',
          default_features: (t.name || '').includes('图片')
            ? imageFeatureOptions(parseFeatureAttrList(t.default_features))
            : parseFeatureAttrList(t.default_features),
        })));
      }
      if (res.default_model_mids) {
        let midsArray: string[] = [];
        if (Array.isArray(res.default_model_mids)) {
          midsArray = res.default_model_mids;
        } else if (typeof res.default_model_mids === 'object' && res.default_model_mids !== null) {
          midsArray = Object.values(res.default_model_mids).filter(v => typeof v === 'string') as string[];
        }
        setPgDefaultModelMids(midsArray);
      }
      if (res.advanced_nodes) {
        setPgAdvancedNodesEnabled(!!res.advanced_nodes.enabled);
        setPgAdvancedNodePreviewEnabled(res.advanced_nodes.preview_enabled !== false);
        setPgAdvancedNodeVolcEnhanceEnabled(!!res.advanced_nodes.volc_enhance_enabled);
        setPgAdvancedNodeDirectorEnabled(!!res.advanced_nodes.director_enabled);
        setPgAdvancedNodePromptEnabled(res.advanced_nodes.prompt_enabled !== false);
        setPgAdvancedNodeAiVideoEnabled(res.advanced_nodes.ai_video_enabled !== false);
        setPgAdvancedNodeAiImageEnabled(res.advanced_nodes.ai_image_enabled !== false);
        setPgAdvancedNodeAgentEnabled(!!res.advanced_nodes.agent_enabled);
        setPgAdvancedNodesUnifiedLimitEnabled(res.advanced_nodes.unified_limit_enabled ?? false);
        setPgAdvancedNodesUnifiedLimitValue(res.advanced_nodes.unified_limit_value ?? 10);
        setPgAdvancedNodePreviewLimit(res.advanced_nodes.preview_limit ?? 10);
        setPgAdvancedNodePromptLimit(res.advanced_nodes.prompt_limit ?? 10);
        setPgAdvancedNodeAiVideoLimit(res.advanced_nodes.ai_video_limit ?? 10);
        setPgAdvancedNodeAiImageLimit(res.advanced_nodes.ai_image_limit ?? 10);
        setPgAdvancedNodeAgentLimit(res.advanced_nodes.agent_limit ?? 10);
        setPgAdvancedNodeVolcEnhanceLimit(res.advanced_nodes.volc_enhance_limit ?? 10);
        setPgAdvancedNodeInstanceLimit(res.advanced_nodes.instance_limit ?? 50);
        setPgAgentModeEnabled(!!res.advanced_nodes.agent_mode_enabled);
        setPgAgentVideoMode(res.advanced_nodes.agent_video_mode || 'track');
        setPgAgentWelcomeTitle(res.advanced_nodes.agent_welcome_title || '');
        setPgAgentWelcomeDesc(res.advanced_nodes.agent_welcome_desc || '');
        setPgAgentSystemPrompt(res.advanced_nodes.agent_system_prompt || '');
        setPgAgentChatModels(res.advanced_nodes.agent_chat_models || []);
        if (res.advanced_nodes.agent_preset_prompts && Array.isArray(res.advanced_nodes.agent_preset_prompts)) {
          setPgAgentPresetPrompts(res.advanced_nodes.agent_preset_prompts);
        } else {
          setPgAgentPresetPrompts([]);
        }
        setVolcEnhancePluginActive(!!res.advanced_nodes.volc_enhance_plugin_active);
      }
    } catch (e) {
      console.error(e);
    }
  };

  const fetchPgClassificationsStats = async () => {
    try {
      const params = buildClassificationParams(pgSelectedProvider, pgSelectedApiProvider, pgSelectedType);
      const resp = await (request.get('/classifications/stats', { params }) as any);
      setPgProvidersStats(resp.providers || []);
      setPgApiProvidersStats(resp.api_providers || []);
      setPgTypesStats(resp.types || []);
    } catch (e) {
      console.error(e);
    }
  };

  const [savingHa, setSavingHa] = useState(false);
  const [clearingHaRuntime, setClearingHaRuntime] = useState(false);
  const fetchHaConfigBase = async () => {
    try {
      const res = await (request.get(`/plugins/${name}/ha-config`) as Promise<any>);
      const rules: HaRuleDto[] = Array.isArray(res?.rules) && res.rules.length ? res.rules : [newHaRule()];
      const def = rules.some(r => r.id === res?.def) ? res.def : rules[0].id;
      setHaRules(rules);
      setHaDef(def);
      setHaDraft(null);
    } catch (e) {
      console.error(e);
    }
  };

  const openHaCreate = () => {
    if (haRules.length >= HA_MAX_RULES) {
      message.warning(`最多 ${HA_MAX_RULES} 条规则`);
      return;
    }
    setHaDraftIsNew(true);
    setHaDraft(newHaRule());
  };

  const openHaEdit = (r: HaRuleDto) => {
    setHaDraftIsNew(false);
    setHaDraft({
      ...r,
      melt: { ...(r.melt || {}) },
      allow: [...(r.allow || [])],
      deny: [...(r.deny || [])],
    });
  };

  const openHaCopy = (r: HaRuleDto) => {
    if (haRules.length >= HA_MAX_RULES) {
      message.warning(`最多 ${HA_MAX_RULES} 条规则`);
      return;
    }
    setHaCopySource(r);
    setHaCopyName(`${r.name} 副本`);
  };

  const patchHaDraft = (fn: (r: HaRuleDto) => HaRuleDto) => {
    setHaDraft(d => (d ? fn(d) : d));
  };

  const setHaDraftMelt = (key: string, secs: number) => {
    patchHaDraft(r => {
      const melt = { ...r.melt };
      if (secs > 0) melt[key] = secs;
      else delete melt[key];
      return { ...r, melt };
    });
  };

  const setHaDraftAuthMelt = (secs: number) => {
    patchHaDraft(r => {
      const melt = { ...r.melt };
      if (secs > 0) {
        melt['401'] = secs;
        melt['402'] = secs;
      } else {
        delete melt['401'];
        delete melt['402'];
      }
      return { ...r, melt };
    });
  };

  const commitHaDraft = async () => {
    if (!haDraft) return;
    const nameTrim = (haDraft.name || '').trim();
    if (!nameTrim) {
      message.error('请填写规则名称');
      return;
    }
    const next: HaRuleDto = {
      ...haDraft,
      name: nameTrim,
      budget: haDraft.budget > 0 ? haDraft.budget : 900,
      ttfb: typeof haDraft.ttfb === 'number' && haDraft.ttfb >= 0 ? haDraft.ttfb : 0,
    };
    let nextRules: HaRuleDto[];
    if (haDraftIsNew) {
      if (haRules.some(r => r.id === next.id)) {
        message.error('规则 id 冲突，请重试');
        return;
      }
      nextRules = [...haRules, next];
    } else {
      nextRules = haRules.map(r => (r.id === next.id ? next : r));
    }

    try {
      setSavingHa(true);
      await request.post(`/plugins/${name}/ha-config`, { def: haDef, rules: nextRules });
      await request.post(`/plugins/${name}/ha-runtime/clear`);
      message.success(haDraftIsNew ? '新建高可用规则成功' : '编辑高可用规则成功');
      setHaRules(nextRules);
      setHaDraft(null);
      await fetchHaConfigBase();
    } catch (e: any) {
      console.error(e);
    } finally {
      setSavingHa(false);
    }
  };

  const commitHaCopy = async () => {
    if (!haCopySource || savingHa) return;
    if (haRules.length >= HA_MAX_RULES) {
      message.warning(`最多 ${HA_MAX_RULES} 条规则`);
      return;
    }
    const nameTrim = (haCopyName || '').trim();
    if (!nameTrim) {
      message.error('请填写规则名称');
      return;
    }
    const next: HaRuleDto = {
      ...haCopySource,
      id: newHaRuleId(),
      name: nameTrim,
      melt: { ...(haCopySource.melt || {}) },
      allow: [...(haCopySource.allow || [])],
      deny: [...(haCopySource.deny || [])],
    };
    if (haRules.some(r => r.id === next.id)) {
      message.error('规则 id 冲突，请重试');
      return;
    }
    const nextRules = [...haRules, next];
    try {
      setSavingHa(true);
      await request.post(`/plugins/${name}/ha-config`, { def: haDef, rules: nextRules });
      await request.post(`/plugins/${name}/ha-runtime/clear`);
      message.success('复制规则成功');
      setHaRules(nextRules);
      setHaCopySource(null);
      await fetchHaConfigBase();
    } catch (e: any) {
      console.error(e);
    } finally {
      setSavingHa(false);
    }
  };

  const setHaDefault = async (id: string) => {
    if (!haRules.some(r => r.id === id)) return;
    try {
      setSavingHa(true);
      await request.post(`/plugins/${name}/ha-config`, { def: id, rules: haRules });
      await request.post(`/plugins/${name}/ha-runtime/clear`);
      setHaDef(id);
      message.success('已设为默认规则');
      await fetchHaConfigBase();
    } catch (e: any) {
      console.error(e);
    } finally {
      setSavingHa(false);
    }
  };

  const deleteHaRule = async (id: string) => {
    if (haRules.length <= 1) {
      message.warning('至少保留一条规则');
      return;
    }
    if (id === haDef) {
      message.warning('请先将其他规则设为默认后再删除');
      return;
    }
    const nextRules = haRules.filter(r => r.id !== id);
    try {
      setSavingHa(true);
      await request.post(`/plugins/${name}/ha-config`, { def: haDef, rules: nextRules });
      await request.post(`/plugins/${name}/ha-runtime/clear`);
      setHaRules(nextRules);
      message.success('删除规则成功');
      await fetchHaConfigBase();
    } catch (e: any) {
      console.error(e);
    } finally {
      setSavingHa(false);
    }
  };

  const handleClearHaRuntime = async () => {
    try {
      setClearingHaRuntime(true);
      await request.post(`/plugins/${name}/ha-runtime/clear`);
      message.success('运行时缓存已清除');
    } catch (e: any) {
      console.error(e);
    } finally {
      setClearingHaRuntime(false);
    }
  };

  const isPlaygroundLike = name === 'playground' || name === 'playground_2026';

  useEffect(() => {
    if (isPlaygroundLike) {
      fetchPlaygroundConfigBase();
    } else if (name === 'high_availability_channel') {
      fetchHaConfigBase();
    }
  }, [name]);

  // Playground: 等级被选中时，将其配额初始化为当前全局默认值的快照（确保等级配额与全局默认互不影响）
  useEffect(() => {
    if (!isPlaygroundLike || isAllLevels || levels.length === 0) return;
    let changed = false;
    const nq = { ...levelQuotas }, np = { ...levelMaxProjects }, na = { ...levelMaxAssets };
    for (const lv of levels) {
      const key = lv.id.toString();
      if (!selectedLevels.includes(key) && !selectedLevels.includes(lv.group_key)) continue;
      if (nq[key] == null) { nq[key] = defaultQuota; changed = true; }
      if (np[key] == null) { np[key] = defaultMaxProjects; changed = true; }
      if (na[key] == null) { na[key] = defaultMaxAssets; changed = true; }
    }
    if (changed) {
      setLevelQuotas(nq);
      setLevelMaxProjects(np);
      setLevelMaxAssets(na);
    }
  }, [selectedLevels, levels]);

  useEffect(() => {
    if (isPlaygroundLike) {
      fetchPgClassificationsStats();
    }
  }, [name, pgSelectedProvider, pgSelectedApiProvider, pgSelectedType]);

  const buildPlaygroundModelsPayload = (models: any[]) => models.map(m => ({
    id: m.id,
    mid: m.mid || '',
    enabled: m.pg_enabled,
    scheme_id: m.pg_scheme_id || null,
    param_overrides: m.pg_param_overrides || null,
    io_overrides: m.pg_io_overrides || null,
    sort_order: m.pg_sort_order || 0,
    ...(name === 'playground_2026' && featureKindFromTypeName(m.type_name) ? {
      feature_attributes: parseFeatureAttrList(m.feature_attributes),
      feature_keys: Array.isArray(m.pg_feature_keys) ? m.pg_feature_keys : [],
    } : {}),
    ...(name === 'playground_2026' && featureKindFromTypeName(m.type_name) === 'image' && m.pg_image_special_params && typeof m.pg_image_special_params === 'object' ? {
      image_special_params: m.pg_image_special_params,
    } : {}),
    ...(name === 'playground_2026' && featureKindFromTypeName(m.type_name) === 'chat' ? {
      ...(typeof m.pg_input_protocol === 'string' && m.pg_input_protocol
        ? { input_protocol: m.pg_input_protocol }
        : {}),
      ...(typeof m.pg_thinking_profile === 'string' && m.pg_thinking_profile
        ? { thinking_profile: m.pg_thinking_profile }
        : {}),
    } : {}),
    ...(name === 'playground_2026' && (featureKindFromTypeName(m.type_name) === 'image' || featureKindFromTypeName(m.type_name) === 'video') ? {
      prompt_optimize: m.pg_prompt_optimize && typeof m.pg_prompt_optimize === 'object' ? m.pg_prompt_optimize : null,
    } : {}),
    ...(name === 'playground_2026' && pgSchemes.find((s: any) => s.id === m.pg_scheme_id)?.type === 'audio' ? {
      voice_library: m.pg_voice_library && typeof m.pg_voice_library === 'object' ? m.pg_voice_library : null,
    } : {}),
    ...(name === 'playground_2026' ? {
      ...(typeof m.mp_enabled === 'boolean' ? { mp_enabled: m.mp_enabled } : {}),
      ...(Array.isArray(m.mp_level_ids) ? { mp_level_ids: m.mp_level_ids.map((lv: any) => normalizeMpLevelId(lv)) } : {}),
    } : {}),
  }));

  /** 创作中心 2026：方案/调参/IO/特性确认后立刻落库，不再依赖「保存全部配置」 */
  const persistPlaygroundModels = async (
    nextModels: any[],
    extra?: { defaultModelMids?: string[]; successMessage: string },
  ) => {
    try {
      setSavingPlayground(true);
      await request.post(`/plugins/${name}/playground-config`, {
        default_model_mids: extra?.defaultModelMids ?? pgDefaultModelMids,
        models: buildPlaygroundModelsPayload(nextModels),
      });
      invalidateCoalesce('pg2026:public-config-and-user-configs');
      message.success(extra?.successMessage || '已保存');
      return true;
    } catch (e) {
      console.error(e);
      message.error('保存失败');
      return false;
    } finally {
      setSavingPlayground(false);
    }
  };

  const handleSavePlaygroundConfig = async () => {
    try {
      setSavingPlayground(true);
      const payload = {
        default_model_mids: pgDefaultModelMids,
        models: buildPlaygroundModelsPayload(pgModels),
        advanced_nodes: name === 'playground_2026'
          ? {
              // 2026 已移除「高级节点配置 / AI智能体配置」Tab：节点能力默认开启；限额改走 workflow_node_limit
              enabled: true,
              preview_enabled: true,
              volc_enhance_enabled: pgAdvancedNodeVolcEnhanceEnabled,
              director_enabled: pgAdvancedNodeDirectorEnabled,
              prompt_enabled: true,
              ai_video_enabled: true,
              ai_image_enabled: true,
              agent_enabled: false,
            }
          : {
              enabled: pgAdvancedNodesEnabled,
              preview_enabled: pgAdvancedNodePreviewEnabled,
              volc_enhance_enabled: pgAdvancedNodeVolcEnhanceEnabled,
              prompt_enabled: pgAdvancedNodePromptEnabled,
              ai_video_enabled: pgAdvancedNodeAiVideoEnabled,
              ai_image_enabled: pgAdvancedNodeAiImageEnabled,
              agent_enabled: pgAdvancedNodeAgentEnabled,
              unified_limit_enabled: pgAdvancedNodesUnifiedLimitEnabled,
              unified_limit_value: pgAdvancedNodesUnifiedLimitValue,
              preview_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodePreviewLimit,
              prompt_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodePromptLimit,
              ai_video_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAiVideoLimit,
              ai_image_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAiImageLimit,
              agent_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAgentLimit,
              volc_enhance_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeVolcEnhanceLimit,
              instance_limit: pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeInstanceLimit,
              agent_mode_enabled: pgAgentModeEnabled,
              agent_video_mode: pgAgentVideoMode,
              agent_welcome_title: pgAgentWelcomeTitle,
              agent_welcome_desc: pgAgentWelcomeDesc,
              agent_system_prompt: pgAgentSystemPrompt,
              agent_preset_prompts: pgAgentPresetPrompts,
              agent_chat_models: pgAgentChatModels,
            },
      };
      await request.post(`/plugins/${name}/playground-config`, payload);
      invalidateCoalesce('pg2026:public-config-and-user-configs');
      message.success('创作配置保存成功');
    } catch (e) {
      console.error(e);
    } finally {
      setSavingPlayground(false);
    }
  };

  const handlePgToggle = async (id: number, enabled: boolean) => {
    const next = pgModels.map(m => m.id === id ? { ...m, pg_enabled: enabled } : m);
    setPgModels(next);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: enabled ? pg2026T('admin_creation_enabled', '已启用创作') : pg2026T('admin_creation_disabled', '已关闭创作') });
      if (!ok) setPgModels(pgModels);
    }
  };

  const handlePgSortChange = async (id: number, sort: number) => {
    const prev = pgModels;
    const next = pgModels.map(m => m.id === id ? { ...m, pg_sort_order: sort } : m);
    setPgModels(next);
    try {
      await persistModelPageSort(id, sort);
      message.success(name === 'playground_2026' ? pg2026T('admin_sort_saved', '排序已保存') : '排序已保存');
    } catch (e) {
      console.error(e);
      message.error(name === 'playground_2026' ? pg2026T('admin_sort_save_failed', '保存失败') : '保存失败');
      setPgModels(prev);
    }
  };

  const handleSaveInlineSort = async (id: number) => {
    await handlePgSortChange(id, editingSortValue);
    setEditingSortModelId(null);
  };

  const handleOpenSchemeDrawer = (id: number, currentSchemeId: string) => {
    setPgCurrentId(id);
    setPgSelectedSchemeId(currentSchemeId || '');
    setPgSchemeDrawerVisible(true);
  };

  const handleConfirmScheme = async (schemeId?: string) => {
    const nextSchemeId = schemeId !== undefined ? schemeId : pgSelectedSchemeId;
    const next = pgModels.map(m => m.id === pgCurrentId
      ? {
          ...m,
          pg_scheme_id: nextSchemeId,
          ...(name === 'playground_2026' ? { pg_prompt_optimize: null, pg_voice_library: null } : {}),
        }
      : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, {
        successMessage: nextSchemeId ? pg2026T('admin_bind_succ', '方案已绑定') : pg2026T('admin_unbind_succ', '已取消绑定'),
      });
      if (!ok) return;
    }
    setPgModels(next);
    setPgSchemeDrawerVisible(false);
  };

  const handleUnbindScheme = async (modelId: number) => {
    const next = pgModels.map(m => m.id === modelId
      ? {
          ...m,
          pg_scheme_id: '',
          pg_param_overrides: null,
          pg_io_overrides: null,
          pg_image_special_params: {},
          ...(name === 'playground_2026' ? { pg_prompt_optimize: null, pg_voice_library: null } : {}),
        }
      : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_unbind_succ', '已取消绑定方案') });
      if (!ok) return;
    }
    setPgModels(next);
  };

  const inheritPgIspDraft = (record: any) => {
    const scheme = pgSchemes.find((s: any) => s.id === record?.pg_scheme_id);
    if (isImageSpecialForcedOn(scheme?.id) || scheme?.image_special_params?.enabled) {
      return seedImageSpecialParams(scheme.image_special_params);
    }
    return { enabled: false };
  };

  const initPgIspDraft = (record: any) => {
    const scheme = pgSchemes.find((s: any) => s.id === record?.pg_scheme_id);
    const forced = isImageSpecialForcedOn(scheme?.id);
    const own = record?.pg_image_special_params;
    if (own && typeof own === 'object' && !Array.isArray(own) && Object.keys(own).length) {
      if (forced || own.enabled) return seedImageSpecialParams(own);
      return { ...normalizeImageSpecialParams(own), enabled: false };
    }
    return inheritPgIspDraft(record);
  };

  const isPg2026ImageModel = (record: any) =>
    name === 'playground_2026' && featureKindFromTypeName(record?.type_name) === 'image';

  const handleResetParamOverrides = async () => {
    const ovModel = pgModels.find(m => m.id === pgOverrideModelId);
    const resetIsp = isPg2026ImageModel(ovModel);
    const boundScheme = pgSchemes.find((s: any) => s.id === ovModel?.pg_scheme_id);
    const systemScheme = name === 'playground_2026'
      ? defaultSchemeList.find((s: any) => s.id === ovModel?.pg_scheme_id)
      : undefined;
    const towardDefault = name === 'playground_2026' && systemScheme?.params
      ? buildParamOverridesToward(boundScheme?.params, systemScheme.params)
      : null;
    let ispPatch: Record<string, any> = {};
    if (resetIsp) {
      const defIsp = systemScheme?.image_special_params;
      ispPatch = {
        pg_image_special_params: defIsp && typeof defIsp === 'object' && !Array.isArray(defIsp)
          ? seedImageSpecialParams(defIsp)
          : {},
      };
    }
    const next = pgModels.map(m => m.id === pgOverrideModelId
      ? { ...m, pg_param_overrides: towardDefault, ...ispPatch, ...(name === 'playground_2026' && (boundScheme?.type === 'image' || boundScheme?.type === 'video') ? { pg_prompt_optimize: null } : {}), ...(name === 'playground_2026' && boundScheme?.type === 'audio' ? { pg_voice_library: null } : {}) }
      : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_reset_system_succ', '已重置为系统默认方案') });
      if (!ok) return;
    }
    setPgOverrideData(towardDefault || { modify: {}, remove: [], add: [] });
    if (resetIsp) {
      const ispVal = ispPatch.pg_image_special_params;
      if (ispVal && typeof ispVal === 'object' && !Array.isArray(ispVal) && Object.keys(ispVal).length) {
        setPgIspDraft(seedImageSpecialParams(ispVal));
      } else if (systemScheme) {
        setPgIspDraft(
          isImageSpecialForcedOn(systemScheme.id) || systemScheme.image_special_params?.enabled
            ? seedImageSpecialParams(systemScheme.image_special_params)
            : { enabled: false },
        );
      } else {
        setPgIspDraft(inheritPgIspDraft(ovModel));
      }
    } else {
      setPgIspDraft(null);
    }
    setPgPoDraft(null);
    setPgIspModalVisible(false);
    setPgModels(next);
    if (name !== 'playground_2026') {
      setPgOverrideModalVisible(false);
      message.info('已清空覆写，保存后生效');
    }
  };

  const handleConfirmParamOverrides = async () => {
    const cleaned = { ...pgOverrideData };
    if (Object.keys(cleaned.modify || {}).length === 0) delete cleaned.modify;
    if ((cleaned.remove || []).length === 0) delete cleaned.remove;
    if ((cleaned.add || []).length === 0) delete cleaned.add;
    const ovModel = pgModels.find(m => m.id === pgOverrideModelId);
    const ovScheme = pgSchemes.find(s => s.id === ovModel?.pg_scheme_id);
    if (name === 'playground_2026' && (ovScheme?.type === 'image' || ovScheme?.type === 'video' || ovScheme?.type === 'audio')) {
      const quickCount = countQuickBarEnabled(ovScheme?.params || [], cleaned);
      if (quickCount > SCHEME_QUICK_BAR_MAX) {
        message.error(pg2026T('admin_override_quick_bar_tip', `快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`, { max: SCHEME_QUICK_BAR_MAX }));
        return;
      }
    }
    const hasOverrides = Object.keys(cleaned).length > 0;
    let ispPatch: Record<string, any> = {};
    if (isPg2026ImageModel(ovModel)) {
      const own = ovModel.pg_image_special_params;
      const hadOwn = own && typeof own === 'object' && !Array.isArray(own) && Object.keys(own).length > 0;
      const draft = (isImageSpecialForcedOn(ovScheme?.id)
        ? seedImageSpecialParams(pgIspDraft)
        : (pgIspDraft || { enabled: false }));
      const sameAsInherit = JSON.stringify(draft) === JSON.stringify(inheritPgIspDraft(ovModel));
      if (hadOwn || !sameAsInherit) {
        ispPatch = { pg_image_special_params: draft };
      }
    }
    let poPatch: Record<string, any> = {};
    if (name === 'playground_2026' && (ovScheme?.type === 'image' || ovScheme?.type === 'video')) {
      poPatch = { pg_prompt_optimize: pgPoDraft && typeof pgPoDraft === 'object' ? pgPoDraft : null };
    }
    let vlPatch: Record<string, any> = {};
    if (name === 'playground_2026' && ovScheme?.type === 'audio') {
      vlPatch = { pg_voice_library: pgVlDraft && typeof pgVlDraft === 'object' ? pgVlDraft : null };
    }
    const next = pgModels.map(m => m.id === pgOverrideModelId
      ? { ...m, pg_param_overrides: hasOverrides ? cleaned : null, ...ispPatch, ...poPatch, ...vlPatch }
      : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_saved', '参数已保存') });
      if (!ok) return;
    }
    setPgModels(next);
    setPgIspModalVisible(false);
    setPgOverrideModalVisible(false);
    if (name !== 'playground_2026') message.success('参数已调整，保存全部配置后生效');
  };

  const handleResetIoOverrides = async () => {
    const next = pgModels.map(m => m.id === pgIoOverrideModelId ? { ...m, pg_io_overrides: null } : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_reset_io_succ', '已重置为方案默认 IO') });
      if (!ok) return;
    }
    setPgIoOverrideData(null);
    setPgModels(next);
    setPgIoOverrideModalVisible(false);
    if (name !== 'playground_2026') message.info('已清空 IO 覆写，保存后生效');
  };

  const handleConfirmIoOverrides = async () => {
    const next = pgModels.map(m => m.id === pgIoOverrideModelId ? { ...m, pg_io_overrides: pgIoOverrideData || null } : m);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_io_saved', 'IO 配置已保存') });
      if (!ok) return;
    }
    setPgModels(next);
    setPgIoOverrideModalVisible(false);
    if (name !== 'playground_2026') message.success('IO 已调整，保存全部配置后生效');
  };

  // ====== 创作方案配置 Tab ======
  const [schemeList, setSchemeList] = useState<any[]>([]);
  const [defaultSchemeList, setDefaultSchemeList] = useState<any[]>([]);
  const [savingSchemes, setSavingSchemes] = useState(false);
  const [schemeEditVisible, setSchemeEditVisible] = useState(false);
  const [editingScheme, setEditingScheme] = useState<any>(null);
  const [editingSchemeIndex, setEditingSchemeIndex] = useState<number>(-1);
  const [schemeIoEditVisible, setSchemeIoEditVisible] = useState(false);
  const [editingIoScheme, setEditingIoScheme] = useState<any>(null);
  const [editingIoSchemeIndex, setEditingIoSchemeIndex] = useState<number>(-1);
  const [imageSpecialModalOpen, setImageSpecialModalOpen] = useState(false);
  const [imageSpecialBaseline, setImageSpecialBaseline] = useState<any>(null);

  const fetchSchemeList = async () => {
    try {
      const res = await (request.get(`/plugins/${name}/playground-schemes`) as Promise<any>);
      if (res.schemes) setSchemeList(res.schemes);
      if (res.defaults) setDefaultSchemeList(res.defaults);
      if (res.voice_catalogs && typeof res.voice_catalogs === 'object') {
        setPgVoiceCatalogs(res.voice_catalogs);
      }
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    if (isPlaygroundLike) {
      fetchSchemeList();
    }
  }, [name]);

  const persistPlaygroundSchemes = async (
    nextSchemes: any[],
    extra?: { successMessage: string },
  ) => {
    try {
      setSavingSchemes(true);
      await request.post(`/plugins/${name}/playground-schemes`, { schemes: nextSchemes });
      message.success(extra?.successMessage || '已保存');
      return true;
    } catch (e) {
      console.error(e);
      message.error('保存失败');
      return false;
    } finally {
      setSavingSchemes(false);
    }
  };

  const handleSaveAllSchemes = async () => {
    try {
      setSavingSchemes(true);
      await request.post(`/plugins/${name}/playground-schemes`, { schemes: schemeList });
      message.success('方案配置已保存');
    } catch (e) {
      console.error(e);
    } finally {
      setSavingSchemes(false);
    }
  };

  const handleAddScheme = () => {
    // 新建方案：编辑抽屉只含基本信息与参数；按类型自动生成初始 IO（随后可在「IO配置」里改）
    const params: any[] = [
      { key: 'ratio', label: '画面比例', type: 'radio', data_type: 'string', options: ['16:9', '9:16', '1:1'], default: '16:9' },
      { key: 'resolution', label: '分辨率', type: 'select', data_type: 'string', options: ['720p', '1080p', '4k'], default: '1080p' },
      { key: 'duration', label: '时长', type: 'slider', data_type: 'integer', min: 1, max: 60, step: 1, default: 5 },
      { key: 'watermark', label: '水印', type: 'switch', data_type: 'boolean', default: false },
      { key: 'return_last_frame', label: '返回最后一帧', type: 'switch', data_type: 'boolean', default: false },
      { key: 'generate_audio', label: '生成音频', type: 'switch', data_type: 'boolean', default: false },
      { key: 'web_search', label: '联网搜索', type: 'switch', data_type: 'boolean', default: false }
    ];
    if (name === 'playground_2026') {
      for (const p of params) {
        if (p.key === 'ratio' || p.key === 'resolution') p.quick = true;
      }
    }
    const newScheme = ensureSchemeIoDefaults({
      id: `custom_${Date.now()}`,
      name: '新建创作方案',
      type: 'video',
      is_system: false,
      description: '请填写方案描述',
      max_reference_images: 7,
      params,
      ...(name === 'playground_2026' ? { prompt_optimize: seedPromptOptimize(null, 'video') } : {}),
    });
    setEditingScheme(JSON.parse(JSON.stringify(newScheme)));
    setEditingSchemeIndex(-1);
    setSchemeEditVisible(true);
  };

  const handleEditScheme = (scheme: any, index: number) => {
    // 编辑：只改基本信息与参数，不带出 IO 编辑器
    const next = JSON.parse(JSON.stringify(scheme));
    if (name === 'playground_2026' && (next.type === 'image' || next.type === 'video')) {
      next.prompt_optimize = seedPromptOptimize(next.prompt_optimize, next.type);
    }
    if (name === 'playground_2026' && next.type === 'audio') {
      next.voice_library = seedVoiceLibrary(next.voice_library, pgVoiceCatalogs);
    }
    if (isImageSpecialForcedOn(next.id)) {
      const raw = next.image_special_params;
      const hasMap = !!(raw?.image_size?.size_map && typeof raw.image_size.size_map === 'object'
        && Object.keys(raw.image_size.size_map).length);
      next.image_special_params = hasMap
        ? { ...seedImageSpecialParams(raw), smart_size: 'auto', enabled: true }
        : officialImageSpecialParams(next.id);
    }
    setEditingScheme(next);
    setEditingSchemeIndex(index);
    setSchemeEditVisible(true);
  };

  const handleOpenSchemeIo = (scheme: any, index: number) => {
    if (
      scheme.type !== 'image' &&
      scheme.type !== 'video' &&
      !(name === 'playground_2026' && (scheme.type === 'audio' || scheme.type === 'chat'))
    ) {
      message.warning(
        name === 'playground_2026' ? '仅图片、视频、音频、聊天方案支持 IO 配置' : '仅图片、视频方案支持 IO 配置',
      );
      return;
    }
    setEditingIoScheme(ensureSchemeIoDefaults(JSON.parse(JSON.stringify(scheme))));
    setEditingIoSchemeIndex(index);
    setSchemeIoEditVisible(true);
  };

  const handleDeleteScheme = async (index: number) => {
    const next = schemeList.filter((_, i) => i !== index);
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundSchemes(next, { successMessage: pg2026T('admin_scheme_deleted', '方案已删除') });
      if (!ok) return;
    }
    setSchemeList(next);
    if (name !== 'playground_2026') message.success('方案已删除，请点击保存生效');
  };

  const handleResetScheme = (id: string, idx: number) => {
    Modal.confirm({
      title: name === 'playground_2026' ? pg2026T('admin_reset_confirm_title', '确认重置') : '确认重置',
      content: name === 'playground_2026'
        ? pg2026T('admin_reset_confirm_content', '是否将该内置方案重置为初始默认（含参数配置与 IO 配置）？确认后立即生效。')
        : '是否将该内置方案重置为初始默认（含参数配置与 IO 配置）？该操作将在您点击「保存全部方案」后生效。',
      okText: name === 'playground_2026' ? pg2026T('admin_confirm', '确认') : undefined,
      cancelText: name === 'playground_2026' ? pg2026T('admin_cancel', '取消') : undefined,
      onOk: async () => {
        const def = defaultSchemeList.find(s => s.id === id);
        if (!def) {
          message.error('无法获取内置默认参数');
          return Promise.reject();
        }
        const newList = [...schemeList];
        newList[idx] = ensureSchemeIoDefaults(JSON.parse(JSON.stringify(def)));
        if (name === 'playground_2026') {
          const ok = await persistPlaygroundSchemes(newList, { successMessage: pg2026T('admin_reset_scheme_succ', '已重置为默认参数与 IO') });
          if (!ok) return Promise.reject();
        }
        setSchemeList(newList);
        if (name !== 'playground_2026') message.success('已重置为默认参数与 IO，请记得保存配置');
      }
    });
  };

  const handleSaveEditingScheme = async () => {
    if (!editingScheme) return;
    // 编辑抽屉不改 IO；新建或类型切换后若为空则补默认 IO
    let next = { ...editingScheme };
    if (next.type === 'image' || next.type === 'video' || (name === 'playground_2026' && next.type === 'audio')) {
      next = ensureSchemeIoDefaults(next);
    }
    if (name === 'playground_2026' && (next.type === 'image' || next.type === 'video' || next.type === 'audio')) {
      const quickCount = (next.params || []).filter((p: any) => p?.quick).length;
      if (quickCount > SCHEME_QUICK_BAR_MAX) {
        message.error(pg2026T('admin_override_quick_bar_tip', `快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`, { max: SCHEME_QUICK_BAR_MAX }));
        return;
      }
    }
    if (name === 'playground_2026' && isImageSpecialForcedOn(next.id)) {
      next.image_special_params = {
        ...seedImageSpecialParams(next.image_special_params),
        smart_size: 'auto',
        enabled: true,
      };
    }
    const refPort = (next.inputs || []).find(
      (p: any) => p?.key === 'reference_images' || p?.bind_key === 'image_urls' || p?.bind_key === 'reference_urls',
    );
    const maxFromIo =
      refPort && refPort.enabled !== false && typeof refPort.max === 'number'
        ? refPort.max
        : undefined;
    const normalized = {
      ...next,
      // 业务参考图上限以 IO「参考图.数量上限」为准；无 IO 时回落旧字段/默认
      max_reference_images:
        typeof maxFromIo === 'number'
          ? maxFromIo
          : typeof next.max_reference_images === 'number'
            ? next.max_reference_images
            : next.type === 'audio'
              ? 0
              : 7,
    };
    const nextList = editingSchemeIndex >= 0
      ? schemeList.map((s, i) => (i === editingSchemeIndex ? {
        ...normalized,
        inputs: normalized.inputs?.length ? normalized.inputs : (s.inputs || normalized.inputs),
        outputs: normalized.outputs?.length ? normalized.outputs : (s.outputs || normalized.outputs),
      } : s))
      : [...schemeList, normalized];
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundSchemes(nextList, {
        successMessage: editingSchemeIndex >= 0 ? pg2026T('admin_scheme_saved', '方案已保存') : pg2026T('admin_scheme_created', '方案已创建'),
      });
      if (!ok) return;
    }
    setSchemeList(nextList);
    setSchemeEditVisible(false);
    setImageSpecialModalOpen(false);
    if (name !== 'playground_2026') message.success('方案已更新，请点击保存生效');
  };

  const handleSaveSchemeIo = async () => {
    if (!editingIoScheme || editingIoSchemeIndex < 0) return;
    const ioErr = validateSchemeIoForSave(editingIoScheme);
    if (ioErr) {
      message.error(ioErr);
      return;
    }
    const refPort = (editingIoScheme.inputs || []).find(
      (p: any) => p?.key === 'reference_images' || p?.bind_key === 'image_urls' || p?.bind_key === 'reference_urls',
    );
    let maxRef =
      typeof editingIoScheme.max_reference_images === 'number'
        ? editingIoScheme.max_reference_images
        : 7;
    if (refPort && refPort.enabled !== false && typeof refPort.max === 'number') {
      maxRef = refPort.max;
    }
    const normalized = {
      ...editingIoScheme,
      max_reference_images: maxRef,
    };
    const nextList = schemeList.map((s, i) => (i === editingIoSchemeIndex ? normalized : s));
    if (name === 'playground_2026') {
      const ok = await persistPlaygroundSchemes(nextList, { successMessage: pg2026T('admin_io_saved', 'IO 配置已保存') });
      if (!ok) return;
    }
    setSchemeList(nextList);
    setSchemeIoEditVisible(false);
    if (name !== 'playground_2026') message.success('IO 配置已更新，请点击保存生效');
  };

/** 解析「选项列表」输入：支持纯逗号分割，也支持 value:label 语法（自动规避 16:9 纯比例误判） */
function parseOptionsAndLabels(inputStr: string): { options: (string | number)[]; option_labels: Record<string, string> } {
  const options: (string | number)[] = [];
  const option_labels: Record<string, string> = {};
  if (!inputStr || !inputStr.trim()) return { options, option_labels };

  const parts = inputStr.split(',').map((s) => s.trim()).filter(Boolean);
  for (const part of parts) {
    const isPureRatio = /^\d+\s*[:：]\s*\d+$/.test(part);
    const ratioWithLabel = part.match(/^(\d+\s*[:：]\s*\d+)[:：](.+)$/);
    if (ratioWithLabel) {
      const val = ratioWithLabel[1].trim();
      const lbl = ratioWithLabel[2].trim();
      options.push(val);
      if (lbl) option_labels[val] = lbl;
    } else if (isPureRatio) {
      options.push(part);
    } else {
      const colonIdx = part.indexOf(':') !== -1 ? part.indexOf(':') : part.indexOf('：');
      if (colonIdx > 0) {
        const val = part.slice(0, colonIdx).trim();
        const lbl = part.slice(colonIdx + 1).trim();
        if (val) {
          options.push(val);
          if (lbl) option_labels[val] = lbl;
        }
      } else {
        options.push(part);
      }
    }
  }
  return { options, option_labels };
}

/** 解析「选项中文描述映射」：支持 opaque:不透明 或 纯中文列表按下标 1-对-1 映射 */
function parseOptionLabelsMapping(
  mappingStr: string,
  options: (string | number)[],
): Record<string, string> {
  const labels: Record<string, string> = {};
  if (!mappingStr || !mappingStr.trim()) return labels;
  const parts = mappingStr.split(',').map((s) => s.trim()).filter(Boolean);
  const hasColon = parts.some((p) => {
    if (/^\d+\s*[:：]\s*\d+$/.test(p)) return false;
    return p.includes(':') || p.includes('：');
  });

  if (hasColon) {
    for (const part of parts) {
      const ratioWithLabel = part.match(/^(\d+\s*[:：]\s*\d+)[:：](.+)$/);
      if (ratioWithLabel) {
        labels[ratioWithLabel[1].trim()] = ratioWithLabel[2].trim();
      } else {
        const colonIdx = part.indexOf(':') !== -1 ? part.indexOf(':') : part.indexOf('：');
        if (colonIdx > 0) {
          const k = part.slice(0, colonIdx).trim();
          const v = part.slice(colonIdx + 1).trim();
          if (k) labels[k] = v;
        }
      }
    }
  } else {
    parts.forEach((label, idx) => {
      if (idx < options.length && label) {
        labels[String(options[idx])] = label;
      }
    });
  }
  return labels;
}

/** 格式化「选项中文描述映射」字符串展示 */
function formatOptionLabelsDisplay(
  optionLabels?: Record<string, string>,
  options?: (string | number)[],
): string {
  if (!optionLabels || Object.keys(optionLabels).length === 0) return '';
  if (Array.isArray(options) && options.length > 0) {
    return options
      .map((opt) => {
        const key = String(opt);
        const label = optionLabels[key];
        return label && label !== key ? `${key}:${label}` : (label || '');
      })
      .filter(Boolean)
      .join(', ');
  }
  return Object.entries(optionLabels)
    .map(([k, v]) => (v && v !== k ? `${k}:${v}` : ''))
    .filter(Boolean)
    .join(', ');
}

  const handleEditingSchemeParamBatchChange = (paramIndex: number, patch: Record<string, any>) => {
    if (!editingScheme) return;
    const newParams = [...editingScheme.params];
    const prev = newParams[paramIndex] || {};
    let next = { ...prev, ...patch };

    if (name === 'playground_2026' && patch.quick === true) {
      if (!isQuickBarEligible({ ...prev, ...next })) {
        message.error('快捷栏仅支持已配置选项的单选或下拉参数');
        return;
      }
      const enabled = newParams.filter((p: any, i: number) => i !== paramIndex && p?.quick).length;
      if (enabled >= SCHEME_QUICK_BAR_MAX) {
        message.error(`快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`);
        return;
      }
    }

    if (name === 'playground_2026' && patch.type && patch.type !== 'radio' && patch.type !== 'select') {
      next.quick = false;
    }
    if (name === 'playground_2026' && patch.options && (!Array.isArray(patch.options) || patch.options.length === 0)) {
      next.quick = false;
    }

    newParams[paramIndex] = next;
    setEditingScheme({ ...editingScheme, params: newParams });
  };

  const handleEditingSchemeParamChange = (paramIndex: number, field: string, value: any) => {
    if (!editingScheme) return;
    const newParams = [...editingScheme.params];
    const prev = newParams[paramIndex] || {};
    let next = { ...prev, [field]: value };

    if (name === 'playground_2026' && field === 'quick' && value === true) {
      if (!isQuickBarEligible({ ...prev, ...next })) {
        message.error('快捷栏仅支持已配置选项的单选或下拉参数');
        return;
      }
      const enabled = newParams.filter((p: any, i: number) => i !== paramIndex && p?.quick).length;
      if (enabled >= SCHEME_QUICK_BAR_MAX) {
        message.error(`快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`);
        return;
      }
    }

    if (name === 'playground_2026' && field === 'type' && value !== 'radio' && value !== 'select') {
      next.quick = false;
    }
    if (name === 'playground_2026' && field === 'options' && (!Array.isArray(value) || value.length === 0)) {
      next.quick = false;
    }

    newParams[paramIndex] = next;
    setEditingScheme({ ...editingScheme, params: newParams });
  };

  const handleAddParam = () => {
    if (!editingScheme) return;
    const newParams = [...editingScheme.params, { key: `param_${Date.now()}`, label: '新参数', type: 'select', data_type: 'string', options: ['选项1'], default: '选项1' }];
    setEditingScheme({ ...editingScheme, params: newParams });
  };

  const handleRemoveParam = (paramIndex: number) => {
    if (!editingScheme) return;
    const newParams = editingScheme.params.filter((_: any, i: number) => i !== paramIndex);
    setEditingScheme({ ...editingScheme, params: newParams });
  };

  const handleMoveParam = (index: number, direction: 'up' | 'down') => {
    if (!editingScheme?.params) return;
    const targetIndex = direction === 'up' ? index - 1 : index + 1;
    if (targetIndex < 0 || targetIndex >= editingScheme.params.length) return;
    const newParams = [...editingScheme.params];
    const temp = newParams[index];
    newParams[index] = newParams[targetIndex];
    newParams[targetIndex] = temp;
    setEditingScheme({ ...editingScheme, params: newParams });
  };

  const fetchApiLogs = async (page = 1, pageSize = apiLogsPageSize, status = logStatusFilter) => {
    try {
      setApiLogsLoading(true);
      const params: any = { page, page_size: pageSize };
      if (logSourceFilter) params.source = logSourceFilter;
      if (logKeyword) params.keyword = logKeyword;
      if (status) params.status = status;
      const res = await (request.get(`/plugins/${name}/api-logs`, { params }) as any);
      if (res.logs) setApiLogs(res.logs);
      if (res.total != null) setApiLogsTotal(res.total);
      setApiLogsPage(res.page || page);
      if (res.uid_map) {
        setAuditUidMap(prev => ({ ...prev, ...res.uid_map }));
      }
    } catch (e) {
      console.error('获取接口日志失败', e);
    } finally {
      setApiLogsLoading(false);
    }
  };

  // 审核日志
  const [auditLogs, setAuditLogs] = useState<any[]>([]);
  const [auditLoading, setAuditLoading] = useState(false);
  const [auditUidMap, setAuditUidMap] = useState<Record<string, { uid: string; username: string }>>({});
  const { page: auditPage, pageSize: auditPageSize, setPage: setAuditPage, setPageSize: setAuditPageSize } = useListPager();

  useEffect(() => {
    setApiLogs([]);
    setApiLogsTotal(0);
    setAuditLogs([]);
    setExpandedAssetInfo({});
    fetchData();
  }, [name]);

  useEffect(() => {
    if (activeTabKey === 'api_log' && !loading && apiLogs.length === 0) {
      fetchApiLogs(1);
    }
    if (activeTabKey === 'audit_log' && !loading) {
      (async () => {
        try {
          setAuditLoading(true);
          const res = await (request.get(`/assets/admin/list?category=虚拟人像`, { headers: { 'x-plugin-ns': name } }) as any);
          if (res.assets) setAuditLogs(res.assets);
          if (res.uid_map) setAuditUidMap(res.uid_map);
        } catch (e) {
          console.error('获取审核日志失败', e);
        } finally {
          setAuditLoading(false);
        }
      })();
    }
  }, [activeTabKey, loading]);

  const fetchData = async () => {
    try {
      setLoading(true);
      const [pluginRes, levelRes, storageRes, moderationRes] = await Promise.all([
        request.get('/plugins') as any,
        request.get('/user_levels') as any,
        request.get(`/plugins/${name}/storage-config`) as any,
        request.get(`/plugins/${name}/moderation-config`) as any,
      ]);

      const found = pluginRes.plugins?.find((p: Plugin) => p.name === name);
      if (found) {
        setPlugin(found);
        setShowInAdminMenu(found.show_in_admin_menu === 1);
        setAdminMenuSort(found.admin_menu_sort ?? 0);
        setAdminMenuTitle(found.admin_menu_title ?? '');
        setAdminMenuDefaultTab(found.admin_menu_default_tab ?? '');
        if (found.category === 'system' || found.category === 'system_builtin') {
          // 系统增强插件：解析管理员等级权限
          if (found.allowed_levels === 'all') {
            setIsAllAdminGroups(true);
            setSelectedAdminGroups([]);
          } else {
            setIsAllAdminGroups(false);
            setSelectedAdminGroups(found.allowed_levels.split(',').filter(Boolean).map(Number).filter((n: number) => !isNaN(n)));
          }
        } else {
          if (found.allowed_levels === 'all') {
            setIsAllLevels(true);
            setSelectedLevels([]);
          } else {
            setIsAllLevels(false);
            setSelectedLevels(found.allowed_levels.split(',').filter(Boolean));
          }
        }
      }

      const allLevels = Array.isArray(levelRes) ? levelRes : (levelRes.data || levelRes.levels || []);
      setLevels(allLevels);

      // 系统增强插件：加载管理员等级
      try {
        const agRes = await (request.get('/admin_groups') as any);
        const groups = agRes?.data || agRes || [];
        if (Array.isArray(groups)) setAdminGroups(groups);
      } catch { /* 忽略 */ }

      if (storageRes) {
        setStorageConfig(storageRes);
        // 加载等级配额 (统一使用 ULID key 即 lv.id.toString())
        if (storageRes.level_quotas) {
          const lq: Record<string, number> = {};
          allLevels.forEach((lv: any) => {
            const key = lv.id.toString();
            lq[key] = storageRes.level_quotas[key] ?? storageRes.level_quotas[lv.group_key];
          });
          setLevelQuotas(lq);
        }
        if (storageRes.default_quota != null) {
          setDefaultQuota(storageRes.default_quota);
        }
        if (storageRes.level_max_folders) {
          const lmf: Record<string, number> = {};
          allLevels.forEach((lv: any) => {
            const key = lv.id.toString();
            lmf[key] = storageRes.level_max_folders[key] ?? storageRes.level_max_folders[lv.group_key];
          });
          setLevelMaxFolders(lmf);
        }
        if (storageRes.default_max_folders != null) {
          setDefaultMaxFolders(storageRes.default_max_folders);
        }
        if (storageRes.level_max_files_per_folder) {
          const lmfpf: Record<string, number> = {};
          allLevels.forEach((lv: any) => {
            const key = lv.id.toString();
            lmfpf[key] = storageRes.level_max_files_per_folder[key] ?? storageRes.level_max_files_per_folder[lv.group_key];
          });
          setLevelMaxFilesPerFolder(lmfpf);
        }
        if (storageRes.default_max_files_per_folder != null) {
          setDefaultMaxFilesPerFolder(storageRes.default_max_files_per_folder);
        }

        if (storageRes.level_max_projects) {
          const lmp: Record<string, number> = {};
          allLevels.forEach((lv: any) => {
            const key = lv.id.toString();
            lmp[key] = storageRes.level_max_projects[key] ?? storageRes.level_max_projects[lv.group_key];
          });
          setLevelMaxProjects(lmp);
        }
        if (storageRes.default_max_projects != null) setDefaultMaxProjects(storageRes.default_max_projects);

        if (storageRes.level_max_assets) {
          const lma: Record<string, number> = {};
          allLevels.forEach((lv: any) => {
            const key = lv.id.toString();
            lma[key] = storageRes.level_max_assets[key] ?? storageRes.level_max_assets[lv.group_key];
          });
          setLevelMaxAssets(lma);
        }
        if (storageRes.default_max_assets != null) setDefaultMaxAssets(storageRes.default_max_assets);
        if (storageRes.workflow_node_limit != null) {
          setWorkflowNodeLimit(Number(storageRes.workflow_node_limit) || 200);
        }
        if (name === 'playground_2026') {
          setUserNavModules(normalizeUserNavModules(storageRes.user_nav_modules, {
            audioEnabled: storageRes.audio_enabled,
            workflowEnabled: storageRes.workflow_enabled,
            workflowMenuTitle: storageRes.workflow_menu_title,
          }));
          setSiteDefaultEnabled(storageRes.site_default_enabled === true);
        } else if (storageRes.workflow_enabled != null) {
          setUserNavModules(patchUserNavModule(defaultUserNavModules(), 'workflow', {
            enabled: !!storageRes.workflow_enabled,
            title: storageRes.workflow_menu_title || '',
          }));
        }
        if (storageRes.show_in_playground_prompt != null) setShowInPlaygroundPrompt(storageRes.show_in_playground_prompt);
        if (storageRes.docs_api_allow_guest != null) {
          setDocsApiAllowGuest(storageRes.docs_api_allow_guest);
          if (name === 'model_marketplace') setMpAllowGuest(storageRes.docs_api_allow_guest);
        }

        // 延迟设置表单值，等待 Tabs 内的 Form 组件渲染完毕
        setTimeout(() => {
          storageForm.setFieldsValue({
            default_provider: storageRes.default_provider || 'tos',
            tos_access_key: storageRes.tos_access_key || '',
            tos_secret_key: storageRes.tos_secret_key || '',
            tos_endpoint: storageRes.tos_endpoint || '',
            tos_region: storageRes.tos_region || '',
            tos_bucket: storageRes.tos_bucket || '',
            tos_path_prefix: storageRes.tos_path_prefix || '',
            tos_custom_domain: storageRes.tos_custom_domain || '',
            cos_secret_id: storageRes.cos_secret_id || '',
            cos_secret_key: storageRes.cos_secret_key || '',
            cos_endpoint: storageRes.cos_endpoint || '',
            cos_region: storageRes.cos_region || '',
            cos_bucket: storageRes.cos_bucket || '',
            cos_path_prefix: storageRes.cos_path_prefix || '',
            cos_custom_domain: storageRes.cos_custom_domain || '',
          });
        }, 0);
      }

      if (moderationRes) {
        setModerationConfig(moderationRes);
        setTimeout(() => {
          moderationForm.setFieldsValue({
            volc_access_key: moderationRes.volc_access_key || '',
            volc_secret_key: moderationRes.volc_secret_key || '',
            volc_app_id: moderationRes.volc_app_id || '',
            volc_project_name: moderationRes.volc_project_name || 'default',
            volc_group_id: moderationRes.volc_group_id || '',
            volc_region: moderationRes.volc_region || 'cn-beijing'
          });
        }, 0);
      }
    } catch (error) {
      // 全局拦截器已统一弹出错误提示
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (checked: boolean) => {
    if (!plugin) return;
    try {
      await request.post(`/plugins/${plugin.name}/toggle`, { is_enabled: checked ? 1 : 0 });
      invalidateAdminPluginsCache();
      invalidateActivePluginsCache();
      message.success(checked ? '插件已开启' : '插件已关闭');
      fetchData();
    } catch (error) {
      // 全局拦截器已统一弹出错误提示
    }
  };

  // ================= 辅助快捷选择逻辑 =================
  const handleSelectAllLevels = () => {
    const allLvs = levels.map(lv => lv.id.toString());
    setSelectedLevels(allLvs);
  };

  const handleClearAllLevels = () => {
    setSelectedLevels([]);
  };

  const handleSelectAllAdminGroups = () => {
    const allGroups = adminGroups.map(ag => ag.id);
    setSelectedAdminGroups(allGroups);
  };

  const handleClearAllAdminGroups = () => {
    setSelectedAdminGroups([]);
  };

  const handleSaveBasic = async () => {
    if (!plugin) return;
    let allowed: string;
    if (plugin.category === 'system' || plugin.category === 'system_builtin') {
      // 系统增强插件：保存管理员等级 ID
      allowed = isAllAdminGroups ? 'all' : selectedAdminGroups.join(',');
      if (!isAllAdminGroups && selectedAdminGroups.length === 0) {
        message.warning('请至少选择一个管理员等级');
        return;
      }
    } else {
      allowed = isAllLevels ? 'all' : selectedLevels.join(',');
      if (!isAllLevels && selectedLevels.length === 0) {
        message.warning('请至少选择一个用户等级');
        return;
      }
    }
    try {
      setSaving(true);
      await request.post(`/plugins/${plugin.name}/config`, {
        allowed_levels: allowed,
        level_quotas: levelQuotas,
        default_quota: defaultQuota,
        level_max_folders: levelMaxFolders,
        default_max_folders: defaultMaxFolders,
        level_max_files_per_folder: levelMaxFilesPerFolder,
        default_max_files_per_folder: defaultMaxFilesPerFolder,
        level_max_projects: levelMaxProjects,
        default_max_projects: defaultMaxProjects,
        level_max_assets: levelMaxAssets,
        default_max_assets: defaultMaxAssets,
        ...(name === 'playground_2026'
          ? {
              workflow_node_limit: workflowNodeLimit,
              workflow_enabled: workflowEnabled,
              workflow_menu_title: workflowMenuTitle.trim(),
              volc_enhance_enabled: pgAdvancedNodeVolcEnhanceEnabled,
              director_enabled: pgAdvancedNodeDirectorEnabled,
              audio_enabled: userNavModules.some((m) => m.id === 'audio' && m.enabled),
              site_default_enabled: siteDefaultEnabled,
              user_nav_modules: userNavModules,
            }
          : {}),
        show_in_playground_prompt: showInPlaygroundPrompt,
        docs_api_allow_guest: docsApiAllowGuest,
        show_in_admin_menu: showInAdminMenu ? 1 : 0,
        admin_menu_sort: adminMenuSort,
        admin_menu_title: adminMenuTitle.trim(),
        admin_menu_default_tab: resolvePluginAdminDefaultTab(
          plugin.name,
          adminMenuDefaultTab,
          dynamicPlugins[plugin.name],
        ),
      });
      // 同步本地插件状态，供「API 接口调用」Tab 正确展示开放等级标签
      setPlugin((prev) => (prev ? {
        ...prev,
        allowed_levels: allowed,
        show_in_admin_menu: showInAdminMenu ? 1 : 0,
        admin_menu_sort: adminMenuSort,
        admin_menu_title: adminMenuTitle.trim(),
        admin_menu_default_tab: resolvePluginAdminDefaultTab(
          plugin.name,
          adminMenuDefaultTab,
          dynamicPlugins[plugin.name],
        ),
      } : prev));
      invalidateAdminPluginsCache();
      if (name === 'playground_2026') invalidateActivePluginsCache();
      message.success('配置已保存');
    } catch (error) {
      // 全局拦截器已统一弹出错误提示
    } finally {
      setSaving(false);
    }
  };

  const handleSaveStorage = async () => {
    try {
      const values = storageForm.getFieldsValue(true);
      setSavingStorage(true);
      await request.post(`/plugins/${name}/storage-config`, values);
      message.success('存储配置已保存');
    } catch (error: any) {
      if (error?.errorFields) return;
    } finally {
      setSavingStorage(false);
    }
  };

  // 审核开关状态
  const [reviewEnabled, setReviewEnabled] = useState(false);

  // 同步服务端的 review_enabled 状态
  useEffect(() => {
    if (moderationConfig) {
      setReviewEnabled(moderationConfig.review_enabled === true);
    }
  }, [moderationConfig]);

  const handleSaveModeration = async () => {
    try {
      const values = await moderationForm.validateFields();
      setSavingModeration(true);
      await request.post(`/plugins/${name}/moderation-config`, { ...values, review_enabled: reviewEnabled });
      message.success('审核配置已保存');
    } catch (error: any) {
      if (error?.errorFields) return; // 表单验证失败
      // 全局拦截器已统一弹出错误提示
    } finally {
      setSavingModeration(false);
    }
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 80 }}><Spin size="large" /></div>;
  }

  if (!plugin) {
    return <div style={{ textAlign: 'center', padding: 80 }}><Text type="secondary">插件不存在</Text></div>;
  }

  const isSystemPlugin = plugin.category === 'system' || plugin.category === 'system_builtin';
  const isEnabled = plugin.is_enabled === 1;

  // ====== 基本配置 Tab ======
  const basicTab = (
    <div>
      {/* 启用状态 */}
      <div style={{
        background: _isLight ? '#fff' : '#141414', borderRadius: 8,
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        padding: '16px 20px', marginBottom: 16,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_basic_enable_status', '启用状态')}</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              {isSystemPlugin ? '开启后，有权限的管理员将在管理后台看到此插件' : pg2026T('admin_basic_enable_user_desc', '开启后，符合等级要求的用户将在菜单中看到此功能')}
            </Text>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Tag color={isEnabled ? 'success' : 'default'} style={{ margin: 0 }}>{isEnabled ? pg2026T('admin_basic_status_running', '运行中') : pg2026T('admin_basic_status_stopped', '已停用')}</Tag>
            <Switch checked={isEnabled} onChange={handleToggle} />
          </div>
        </div>

        <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />

        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_basic_left_menu', '左侧菜单显示')}</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              {pg2026T('admin_basic_left_menu_desc', '开启后，管理后台「站点插件」下出现二级菜单，点击可直达本插件配置页。插件启用状态关闭时，此处开启也不会显示入口。')}
            </Text>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Tag color={showInAdminMenu && isEnabled ? 'success' : 'default'} style={{ margin: 0 }}>
              {showInAdminMenu ? (isEnabled ? pg2026T('admin_enabled', '已开启') : pg2026T('admin_basic_enabled_hidden', '已开启（停用中不显示）')) : pg2026T('admin_disabled', '已关闭')}
            </Tag>
            <Switch checked={showInAdminMenu} onChange={setShowInAdminMenu} />
          </div>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16, marginTop: 14 }}>
          {showInAdminMenu && (
            <>
              <div style={{ flex: '1 1 220px', minWidth: 180 }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, display: 'block', marginBottom: 6 }}>{pg2026T('admin_basic_left_menu_title', '左侧菜单名称')}</Text>
                <Input
                  value={adminMenuTitle}
                  maxLength={64}
                  placeholder={plugin.title}
                  onChange={(e) => setAdminMenuTitle(e.target.value)}
                />
              </div>
              <div style={{ flex: '1 1 200px', minWidth: 180 }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, display: 'block', marginBottom: 6 }}>{pg2026T('admin_basic_default_tab', '默认跳转 Tab')}</Text>
                <Select
                  value={resolvePluginAdminDefaultTab(plugin.name, adminMenuDefaultTab, dynamicPlugins[plugin.name])}
                  onChange={setAdminMenuDefaultTab}
                  options={getPluginAdminTabs(plugin.name, dynamicPlugins[plugin.name], (k, fb) => t(k, { defaultValue: fb })).map((tab) => ({
                    value: tab.key,
                    label: tab.label,
                  }))}
                  style={{ width: '100%' }}
                />
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 11, display: 'block', marginTop: 4 }}>{pg2026T('admin_basic_default_tab_hint', '点击左侧菜单后打开该 Tab，默认第一个')}</Text>
              </div>
            </>
          )}
          <div style={{ width: 160 }}>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, display: 'block', marginBottom: 6 }}>{pg2026T('admin_basic_sort_weight', '排序权重')}</Text>
            <InputNumber
              value={adminMenuSort}
              onChange={(v) => setAdminMenuSort(typeof v === 'number' ? v : 0)}
              style={{ width: '100%' }}
            />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 11, display: 'block', marginTop: 4 }}>{pg2026T('admin_basic_sort_weight_hint', '数字越大越靠前（菜单与插件列表共用）')}</Text>
          </div>
        </div>
      </div>

      {name === 'playground_2026' && (
        <div style={{
          background: _isLight ? '#fff' : '#141414', borderRadius: 8,
          border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
          padding: '16px 20px', marginBottom: 16,
        }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <div>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_site_default', '站点默认创作中心')}</Text><br />
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
                {pg2026T('admin_site_default_desc', '开启后，普通用户登录成功会自动进入创作中心 2026 主页。插件未启用，或该用户等级未开放时，仍进入控制台。默认关闭。')}
              </Text>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Tag color={siteDefaultEnabled ? 'success' : 'default'} style={{ margin: 0 }}>{siteDefaultEnabled ? pg2026T('admin_enabled', '已开启') : pg2026T('admin_disabled', '已关闭')}</Tag>
              <Switch checked={siteDefaultEnabled} onChange={setSiteDefaultEnabled} />
            </div>
          </div>
        </div>
      )}

      {name === 'playground_2026' && (
        <PluginModule>
          <PlaygroundUserNavConfig
            items={userNavModules}
            onChange={setUserNavModules}
            isLight={_isLight}
          />
        </PluginModule>
      )}

      {/* 模型创作中心提示词输入窗口加载显示 (仅限素材资产管理插件) */}
      {isEnabled && (name === 'asset_manager' || name === 'asset_manager_intl') && (
        <div style={{
          background: _isLight ? '#fff' : '#141414', borderRadius: 8,
          border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
          padding: '16px 20px', marginBottom: 16,
          display: 'flex', alignItems: 'center', justifyContent: 'space-between'
        }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>模型创作中心提示词输入窗口加载显示</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              开启后，在模型创作中心的提示词输入窗口将会加载并显示此插件功能
            </Text>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Tag color={showInPlaygroundPrompt ? 'success' : 'default'} style={{ margin: 0 }}>{showInPlaygroundPrompt ? '已开启' : '已关闭'}</Tag>
            <Switch checked={showInPlaygroundPrompt} onChange={setShowInPlaygroundPrompt} />
          </div>
        </div>
      )}

      {/* DocsApi / ModelMarketplace 插件专属：免登录访问 */}
      {isEnabled && (name === 'docs_api' || name === 'model_marketplace') && (
        <div style={{
          background: _isLight ? '#fff' : '#141414', borderRadius: 8,
          border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
          padding: '16px 20px', marginBottom: 16,
          display: 'flex', alignItems: 'center', justifyContent: 'space-between'
        }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>不登录可以直接访问</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              开启后，即使在没有登陆状态的情况下，游客也可以直接访问该内容
            </Text>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Tag color={docsApiAllowGuest ? 'success' : 'default'} style={{ margin: 0 }}>{docsApiAllowGuest ? '已开启' : '已关闭'}</Tag>
            <Switch checked={docsApiAllowGuest} onChange={(val) => {
              setDocsApiAllowGuest(val);
              if (name === 'model_marketplace') setMpAllowGuest(val);
            }} />
          </div>
        </div>
      )}

      {/* 系统增强插件：管理员等级权限 */}
      {isSystemPlugin ? (
        <div style={{
          background: _isLight ? '#fff' : '#141414', borderRadius: 8,
          border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px', marginBottom: 16
        }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>管理员等级权限</Text><br />
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>设置哪些管理员等级可以在管理后台看到并管理此插件</Text>
          <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />

          <div
            style={{ padding: '12px 16px', borderRadius: 6, border: isAllAdminGroups ? '1px solid rgba(250,140,22,0.4)' : (_isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)'), background: isAllAdminGroups ? 'rgba(250,140,22,0.06)' : 'transparent', cursor: 'pointer', marginBottom: 8, transition: 'all 0.15s' }}
            onClick={() => { setIsAllAdminGroups(true); }}
          >
            <Checkbox checked={isAllAdminGroups}><Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>所有管理员等级可见</Text></Checkbox>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 12, display: 'block', marginLeft: 24, marginTop: 2 }}>所有管理员等级均可管理此插件</Text>
          </div>
          <div
            style={{ padding: '12px 16px', borderRadius: 6, border: !isAllAdminGroups ? '1px solid rgba(250,140,22,0.4)' : (_isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)'), background: !isAllAdminGroups ? 'rgba(250,140,22,0.06)' : 'transparent', cursor: 'pointer', transition: 'all 0.15s' }}
            onClick={() => setIsAllAdminGroups(false)}
          >
            <Checkbox checked={!isAllAdminGroups}><Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>仅指定管理员等级可见</Text></Checkbox>
          </div>

          {!isAllAdminGroups && (
            <div style={{ marginTop: 14 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 10 }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>
                  已选择 {selectedAdminGroups.length} 个分组
                </Text>
                <div style={{ display: 'flex', alignItems: 'center' }}>
                  <Button 
                    type="link" 
                    size="small" 
                    onClick={handleSelectAllAdminGroups} 
                    style={{ padding: '0 4px', fontSize: 12, height: 'auto', lineHeight: 1, color: '#fa8c16' }}
                  >
                    全选
                  </Button>
                  <Divider type="vertical" style={{ margin: '0 8px', borderColor: _isLight ? 'rgba(0,0,0,0.1)' : 'rgba(255,255,255,0.15)' }} />
                  <Button 
                    type="link" 
                    size="small" 
                    danger 
                    onClick={handleClearAllAdminGroups} 
                    style={{ padding: '0 4px', fontSize: 12, height: 'auto', lineHeight: 1 }}
                  >
                    清除
                  </Button>
                </div>
              </div>
              {adminGroups.map(ag => {
                const isSelected = selectedAdminGroups.includes(ag.id);
                return (
                  <div key={ag.id}
                    style={{ padding: '10px 14px', borderRadius: 6, border: isSelected ? '1px solid rgba(250,140,22,0.3)' : (_isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)'), background: isSelected ? 'rgba(250,140,22,0.04)' : 'transparent', marginBottom: 6, cursor: 'pointer', transition: 'all 0.15s' }}
                    onClick={() => setSelectedAdminGroups(prev => prev.includes(ag.id) ? prev.filter(id => id !== ag.id) : [...prev, ag.id])}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <Checkbox checked={isSelected} />
                      <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>{ag.name}</Text>
                      {ag.description && <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 12 }}>({ag.description})</Text>}
                    </div>
                  </div>
                );
              })}
              {adminGroups.length === 0 && <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', display: 'block', textAlign: 'center', padding: 16, fontSize: 13 }}>暂无管理员等级，请先在「站点设置 → 管理员等级」中创建</Text>}
            </div>
          )}
        </div>
      ) : (
        <>

          {/* 用户增强插件：用户等级 */}
          <div style={{
            background: _isLight ? '#fff' : '#141414', borderRadius: 8,
            border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px', marginBottom: 16
          }}>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_basic_open_levels', '开放用户等级')}</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>{pg2026T('admin_basic_open_levels_desc', '设置哪些用户等级可以使用此插件功能')}</Text>
            <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />

            <div
              style={{ padding: '12px 16px', borderRadius: 6, border: isAllLevels ? '1px solid rgba(22,119,255,0.4)' : (_isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)'), background: isAllLevels ? 'rgba(22,119,255,0.06)' : 'transparent', cursor: 'pointer', marginBottom: 8, transition: 'all 0.15s' }}
              onClick={() => { setIsAllLevels(true); }}
            >
              <Checkbox checked={isAllLevels}><Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>{pg2026T('admin_basic_all_levels', '对所有用户等级开放')}</Text></Checkbox>
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 12, display: 'block', marginLeft: 24, marginTop: 2 }}>{pg2026T('admin_basic_all_levels_desc', '包含当前及以后新增的所有用户等级')}</Text>
            </div>
            <div
              style={{ padding: '12px 16px', borderRadius: 6, border: !isAllLevels ? '1px solid rgba(22,119,255,0.4)' : (_isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)'), background: !isAllLevels ? 'rgba(22,119,255,0.06)' : 'transparent', cursor: 'pointer', transition: 'all 0.15s' }}
              onClick={() => setIsAllLevels(false)}
            >
              <Checkbox checked={!isAllLevels}><Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>{pg2026T('admin_basic_per_level', '按等级单独设置（覆盖全局默认值）')}</Text></Checkbox>
            </div>

            {!isAllLevels && (
              <div style={{ marginTop: 14 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 10 }}>
                  <Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>
                    {pg2026T('admin_basic_selected_levels_count', `已选择 ${selectedLevels.length} 个等级`, { count: selectedLevels.length })}
                  </Text>
                  <div style={{ display: 'flex', alignItems: 'center' }}>
                    <Button 
                      type="link" 
                      size="small" 
                      onClick={handleSelectAllLevels} 
                      style={{ padding: '0 4px', fontSize: 12, height: 'auto', lineHeight: 1, color: '#1677ff' }}
                    >
                      {pg2026T('common_select_all', '全选')}
                    </Button>
                    <Divider type="vertical" style={{ margin: '0 8px', borderColor: _isLight ? 'rgba(0,0,0,0.1)' : 'rgba(255,255,255,0.15)' }} />
                    <Button 
                      type="link" 
                      size="small" 
                      danger 
                      onClick={handleClearAllLevels} 
                      style={{ padding: '0 4px', fontSize: 12, height: 'auto', lineHeight: 1 }}
                    >
                      {pg2026T('admin_basic_clear', '清除')}
                    </Button>
                  </div>
                </div>
                {levels.map(lv => {
                  const lvIdStr = lv.id.toString();
                  const isSelected = selectedLevels.includes(lvIdStr) || selectedLevels.includes(lv.group_key);
                  const showLimits = name !== 'team_marketing' && name !== 'playground' && name !== 'playground_2026' && name !== 'model_marketplace';
                  return (
                    <div key={lv.group_key}
                      style={{ padding: '10px 14px', borderRadius: 6, border: isSelected ? '1px solid rgba(22,119,255,0.3)' : (_isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)'), background: isSelected ? 'rgba(22,119,255,0.04)' : 'transparent', marginBottom: 6, transition: 'all 0.15s' }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', flex: 1 }}
                          onClick={() => setSelectedLevels(prev => prev.includes(lvIdStr) || prev.includes(lv.group_key) ? prev.filter(k => k !== lvIdStr && k !== lv.group_key) : [...prev, lvIdStr])}
                        >
                          <Checkbox checked={isSelected} />
                          <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>
                            {lv.name}
                            <span style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginLeft: 6 }}>
                              (ULID: {lvIdStr.padStart(4, '0')}{lv.discount != null ? `，${pg2026T('admin_basic_discount_rate', '折扣倍率')}: ${lv.discount}` : ''})
                            </span>
                          </Text>
                        </div>
                      </div>
                      {showLimits && (name === 'asset_manager' || name === 'asset_manager_intl') && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginTop: 8, marginLeft: 24, flexWrap: 'wrap' }} onClick={(e) => e.stopPropagation()}>
                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, whiteSpace: 'nowrap' }}>存储空间</Text>
                            <InputNumber size="small" min={1} max={10240}
                              value={levelQuotas[lv.group_key] ?? 100}
                              onChange={(val) => setLevelQuotas(prev => ({ ...prev, [lv.group_key]: val ?? 100 }))}
                              style={{ width: 72 }}
                              addonAfter="MB"
                            />
                          </div>
                          <div style={{ width: 1, height: 16, background: _isLight ? 'rgba(0,0,0,0.04)' : 'rgba(255,255,255,0.08)' }} />
                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, whiteSpace: 'nowrap' }}>文件夹</Text>
                            <InputNumber size="small" min={1} max={1000}
                              value={levelMaxFolders[lv.group_key] ?? 20}
                              onChange={(val) => setLevelMaxFolders(prev => ({ ...prev, [lv.group_key]: val ?? 20 }))}
                              style={{ width: 68 }}
                              addonAfter="个"
                            />
                          </div>
                          <div style={{ width: 1, height: 16, background: _isLight ? 'rgba(0,0,0,0.04)' : 'rgba(255,255,255,0.08)' }} />
                          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, whiteSpace: 'nowrap' }}>每夹文件</Text>
                            <InputNumber size="small" min={1} max={10000}
                              value={levelMaxFilesPerFolder[lv.group_key] ?? 100}
                              onChange={(val) => setLevelMaxFilesPerFolder(prev => ({ ...prev, [lv.group_key]: val ?? 100 }))}
                              style={{ width: 72 }}
                              addonAfter="个"
                            />
                          </div>
                        </div>
                      )}

                    </div>
                  );
                })}
                {levels.length === 0 && <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', display: 'block', textAlign: 'center', padding: 16, fontSize: 13 }}>{pg2026T('admin_basic_no_levels', '暂无用户等级，请先在「用户管理 → 用户等级」中创建')}</Text>}
              </div>
            )}
          </div>

          {isPlaygroundLike && (
            <div style={{
              background: _isLight ? '#fff' : '#141414', borderRadius: 8,
              border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px', marginBottom: 16
            }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_quota_title', '资源配额管理')}</Text><br />
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>{isAllLevels ? pg2026T('admin_quota_all_desc', '全局默认的模型创作中心配额（对所有用户生效）') : pg2026T('admin_quota_per_level_desc', '按已选等级单独设置资源配额')}</Text>
              <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />

              {/* 表头 */}
              <div style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '0 14px 8px', alignItems: 'center' }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11 }}>{pg2026T('admin_quota_col_level', '等级')}</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>{pg2026T('admin_quota_col_storage', '存储空间')}</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>{pg2026T('admin_quota_col_projects', '项目限制')}</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>{pg2026T('admin_quota_col_assets', '每个项目素材')}</Text>
              </div>

              {/* 全局默认行（仅在对所有用户等级开放时显示） */}
              {isAllLevels && (
                <div style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '10px 14px', borderRadius: 6, border: '1px solid rgba(22,119,255,0.3)', background: 'rgba(22,119,255,0.04)', marginBottom: 8, alignItems: 'center' }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13, fontWeight: 500 }}>{pg2026T('admin_quota_global_default', '全局默认')}</Text>
                    <Tag color="blue" style={{ margin: 0, fontSize: 11 }}>{pg2026T('admin_quota_default_tag', '默认值')}</Tag>
                  </div>
                  <InputNumber size="small" min={1} max={10240}
                    value={defaultQuota} onChange={(val) => setDefaultQuota(val ?? 100)}
                    style={{ width: '100%' }}
                    addonAfter="MB"
                  />
                  <InputNumber size="small" min={1} max={1000}
                    value={defaultMaxProjects} onChange={(val) => setDefaultMaxProjects(val ?? 3)}
                    style={{ width: '100%' }}
                    addonAfter={pg2026T('admin_wf_node_unit', '个')}
                  />
                  <InputNumber size="small" min={1} max={1000}
                    value={defaultMaxAssets} onChange={(val) => setDefaultMaxAssets(val ?? 10)}
                    style={{ width: '100%' }}
                    addonAfter={pg2026T('admin_wf_node_unit', '个')}
                  />
                </div>
              )}

              {/* 按等级覆盖（仅"按等级单独设置"模式且有已选等级时显示） */}
              {!isAllLevels && (() => {
                const selected = levels.filter(lv => selectedLevels.includes(lv.id.toString()) || selectedLevels.includes(lv.group_key));
                return selected.length > 0 ? (
                  <>

                    {selected.map(lv => (
                      <div key={lv.id.toString()}
                        style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '8px 14px', borderRadius: 6, border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)', background: 'transparent', marginBottom: 6, alignItems: 'center' }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                          <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>
                            {lv.name}
                            <span style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginLeft: 6 }}>
                              (ULID: {lv.id.toString().padStart(4, '0')}{lv.discount != null ? `，${pg2026T('admin_basic_discount_rate', '折扣倍率')}: ${lv.discount}` : ''})
                            </span>
                          </Text>
                        </div>
                        <InputNumber size="small" min={1} max={10240}
                          value={levelQuotas[lv.id.toString()] ?? defaultQuota}
                          onChange={(val) => setLevelQuotas(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultQuota }))}
                          style={{ width: '100%' }}
                          addonAfter="MB"
                        />
                        <InputNumber size="small" min={1} max={1000}
                          value={levelMaxProjects[lv.id.toString()] ?? defaultMaxProjects}
                          onChange={(val) => setLevelMaxProjects(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultMaxProjects }))}
                          style={{ width: '100%' }}
                          addonAfter={pg2026T('admin_wf_node_unit', '个')}
                        />
                        <InputNumber size="small" min={1} max={1000}
                          value={levelMaxAssets[lv.id.toString()] ?? defaultMaxAssets}
                          onChange={(val) => setLevelMaxAssets(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultMaxAssets }))}
                          style={{ width: '100%' }}
                          addonAfter={pg2026T('admin_wf_node_unit', '个')}
                        />
                      </div>
                    ))}
                  </>
                ) : null;
              })()}
            </div>
          )}

          {isAllLevels && (name === 'asset_manager' || name === 'asset_manager_intl') && (
            <div style={{
              background: _isLight ? '#fff' : '#141414', borderRadius: 8,
              border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px', marginBottom: 16
            }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>资源配额管理</Text><br />
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>设置每位用户的存储空间、文件夹数量、每文件夹文件数上限，可按等级单独覆盖</Text>
              <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />

              {/* 表头 */}
              <div style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '0 14px 8px', alignItems: 'center' }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11 }}>等级</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>存储空间</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>文件夹上限</Text>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 11, textAlign: 'center' }}>每夹文件上限</Text>
              </div>

              {/* 全局默认行 */}
              <div style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '10px 14px', borderRadius: 6, border: '1px solid rgba(22,119,255,0.3)', background: 'rgba(22,119,255,0.04)', marginBottom: 8, alignItems: 'center' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13, fontWeight: 500 }}>全局默认</Text>
                  <Tag color="blue" style={{ margin: 0, fontSize: 11 }}>默认值</Tag>
                </div>
                <InputNumber size="small" min={1} max={10240}
                  value={defaultQuota} onChange={(val) => setDefaultQuota(val ?? 100)}
                  style={{ width: '100%' }}
                  addonAfter="MB"
                />
                <InputNumber size="small" min={1} max={1000}
                  value={defaultMaxFolders} onChange={(val) => setDefaultMaxFolders(val ?? 20)}
                  style={{ width: '100%' }}
                  addonAfter="个"
                />
                <InputNumber size="small" min={1} max={10000}
                  value={defaultMaxFilesPerFolder} onChange={(val) => setDefaultMaxFilesPerFolder(val ?? 100)}
                  style={{ width: '100%' }}
                  addonAfter="个"
                />
              </div>

              {/* 按等级覆盖 */}
              {levels.length > 0 && (
                <>
                  <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', margin: '12px 0 8px' }}>按等级单独设置（覆盖全局默认值）</Text>
                  {levels.map(lv => (
                    <div key={lv.id.toString()}
                      style={{ display: 'grid', gridTemplateColumns: '1fr repeat(3, 140px)', gap: 8, padding: '8px 14px', borderRadius: 6, border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)', background: 'transparent', marginBottom: 6, alignItems: 'center' }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>
                          {lv.name}
                          <span style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginLeft: 6 }}>
                            (ULID: {lv.id.toString().padStart(4, '0')}{lv.discount != null ? `，折扣倍率: ${lv.discount}` : ''})
                          </span>
                        </Text>
                      </div>
                      <InputNumber size="small" min={1} max={10240}
                        value={levelQuotas[lv.id.toString()] ?? levelQuotas[lv.group_key] ?? defaultQuota}
                        onChange={(val) => setLevelQuotas(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultQuota }))}
                        style={{ width: '100%' }}
                        addonAfter="MB"
                      />
                      <InputNumber size="small" min={1} max={1000}
                        value={levelMaxFolders[lv.id.toString()] ?? defaultMaxFolders}
                        onChange={(val) => setLevelMaxFolders(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultMaxFolders }))}
                        style={{ width: '100%' }}
                        addonAfter="个"
                      />
                      <InputNumber size="small" min={1} max={10000}
                        value={levelMaxFilesPerFolder[lv.id.toString()] ?? defaultMaxFilesPerFolder}
                        onChange={(val) => setLevelMaxFilesPerFolder(prev => ({ ...prev, [lv.id.toString()]: val ?? defaultMaxFilesPerFolder }))}
                        style={{ width: '100%' }}
                        addonAfter="个"
                      />
                    </div>
                  ))}
                </>
              )}
            </div>
          )}
        </>
      )}

      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <Button type="primary" icon={<SaveOutlined />} loading={saving} onClick={handleSaveBasic}>{pg2026T('admin_save_config', '保存配置')}</Button>
      </div>
    </div>
  );

  const inputStyle = {};
  const hasIndependentStorage = !!name && INDEPENDENT_STORAGE_PLUGINS.has(name);
  // 创作中心插件标识（用于展示前端直传 CORS 配置说明）
  const isPlaygroundPlugin = isPlaygroundLike;
  const storageTab = (
    <div>
      {hasIndependentStorage ? (
        <StorageConfigPanel
          form={storageForm}
          namePrefix={[]}
          standalone={true}
          pluginName={name}
          onSaveStandalone={handleSaveStorage}
          savingStandalone={savingStorage}
          maskedSecrets={{
            tos_secret_key: storageConfig?.tos_secret_key_masked || '',
            cos_secret_key: storageConfig?.cos_secret_key_masked || '',
          }}
          globalSnapshot={storageConfig?.global_configured ? {
            provider: storageConfig.global_provider || 'tos',
            bucket: storageConfig.global_bucket || '',
            endpoint: storageConfig.global_endpoint || '',
            region: storageConfig.global_region,
            pathPrefix: storageConfig.global_path_prefix,
          } : undefined}
          siteSettingsHref={`/${adminPath}/settings?tab=database&subtab=storage`}
          extraBottomContent={
            isPlaygroundPlugin ? (
              <div style={{
                marginTop: 16, padding: '16px 20px', borderRadius: 8,
                background: _isLight ? 'rgba(22,119,255,0.03)' : 'rgba(22,119,255,0.06)',
                border: _isLight ? '1px solid rgba(22,119,255,0.15)' : '1px solid rgba(22,119,255,0.2)',
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
                  <InfoCircleOutlined style={{ color: '#1677ff', fontSize: 14 }} />
                  <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13 }}>{pg2026T('admin_cors_title', '前端直传所需的 CORS 配置')}</Text>
                  <Tag color="blue" style={{ margin: 0, fontSize: 11 }}>{pg2026T('admin_cors_tag', '可选但推荐')}</Tag>
                </div>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.55)' : 'rgba(255,255,255,0.55)', fontSize: 12, lineHeight: '20px', display: 'block', marginBottom: 10 }}>
                  {pg2026T('admin_cors_desc', '创作中心支持浏览器直接上传文件到对象存储（无需经过服务器中转，显著节省服务器带宽）。直传请求发往官方域名（与自定义域名 / CDN 解耦，避免签名 Host 不一致）；上传成功后的访问地址仍可使用自定义域名。若需启用此功能，请在 TOS 或 COS 控制台完成以下 CORS 配置，否则上传将自动降级为服务器中转模式。')}
                </Text>
                <div style={{ background: _isLight ? '#f8fafc' : '#1a1a2e', borderRadius: 6, padding: '10px 14px', fontSize: 12 }}>
                  <div style={{ marginBottom: 6 }}>
                    <Text style={{ color: '#1677ff', fontSize: 12 }}>{pg2026T('admin_cors_path_label', '操作路径：')}</Text>
                    <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 12 }}>{pg2026T('admin_cors_path_val', '对象存储控制台 → 选择 Bucket → 权限管理 / 安全管理 → CORS 设置 → 添加规则')}</Text>
                  </div>
                  {[
                    { label: pg2026T('admin_cors_rule_origin', '允许来源 AllowedOrigin'), value: pg2026T('admin_cors_rule_origin_val', '本站完整域名，如 https://your-domain.com（请勿填写 *，存在安全风险）') },
                    { label: pg2026T('admin_cors_rule_methods', '允许方法 AllowedMethod'), value: 'PUT、GET、HEAD' },
                    { label: pg2026T('admin_cors_rule_headers', '允许请求头 AllowedHeader'), value: 'Content-Type、Content-Length' },
                    { label: pg2026T('admin_cors_rule_expose', '暴露响应头 ExposeHeader'), value: 'ETag' },
                    { label: pg2026T('admin_cors_rule_maxage', '预检缓存 MaxAgeSeconds'), value: '3600' },
                  ].map(item => (
                    <div key={item.label} style={{ display: 'flex', gap: 8, marginBottom: 3, alignItems: 'flex-start' }}>
                      <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.4)', fontSize: 11, whiteSpace: 'nowrap', minWidth: 160 }}>{item.label}:</Text>
                      <Text style={{ color: _isLight ? '#1f2937' : '#e2e8f0', fontSize: 11 }}>{item.value}</Text>
                    </div>
                  ))}
                </div>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.3)', fontSize: 11, marginTop: 8, display: 'block' }}>
                  {pg2026T('admin_cors_hint', '💡 配置后用户上传文件将直接传输到 TOS，不再经过服务器，大幅降低带宽消耗。未配置时自动降级为服务器中转，功能不受影响。')}
                </Text>
              </div>
            ) : null
          }
        />
      ) : (
        /* 其它插件继续继承站点全局存储设置 */
        <div style={{
          background: _isLight ? '#fff' : '#141414', borderRadius: 8,
          border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px'
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 16 }}>
            <CloudServerOutlined style={{ color: '#1677ff', fontSize: 16 }} />
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>对象存储配置</Text>
          </div>

          <Alert
            type={storageConfig?.global_configured ? "success" : "warning"}
            showIcon
            icon={storageConfig?.global_configured ? <CheckCircleOutlined /> : <InfoCircleOutlined />}
            message={storageConfig?.global_configured ? "已使用全局存储设置" : "未配置存储"}
            description={
              <div>
                <p style={{ margin: 0 }}>当前存储默认使用管理后台「<Link to={`/${adminPath}/settings?tab=database&subtab=storage`}>站点设置 → 存储设置</Link>」里面的对象存储配置，无需在此处单独进行配置。站点默认厂商变更后，本页会同步显示。</p>
                {storageConfig?.global_configured ? (
                  <div style={{ marginTop: 12, borderTop: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.08)', paddingTop: 8 }}>
                    <p style={{ margin: '4px 0 0 0', fontSize: 12 }}>
                      <b>已启用全局存储设置：</b>
                    </p>
                    <p style={{ margin: '4px 0 0 0', fontSize: 12 }}>
                      全局提供商: <code style={{ background: _isLight ? '#f3f4f6' : '#27272a', padding: '2px 4px', borderRadius: 4, fontFamily: 'monospace' }}>{getStorageProvider(storageConfig.global_provider || 'tos').name}</code>
                    </p>
                    <p style={{ margin: '4px 0 0 0', fontSize: 12 }}>
                      全局 Bucket: <code style={{ background: _isLight ? '#f3f4f6' : '#27272a', padding: '2px 4px', borderRadius: 4, fontFamily: 'monospace' }}>{storageConfig.global_bucket || '—'}</code>
                    </p>
                    {storageConfig.global_endpoint ? (
                      <p style={{ margin: '4px 0 0 0', fontSize: 12 }}>
                        Endpoint: <code style={{ background: _isLight ? '#f3f4f6' : '#27272a', padding: '2px 4px', borderRadius: 4, fontFamily: 'monospace' }}>{storageConfig.global_endpoint}</code>
                      </p>
                    ) : null}
                  </div>
                ) : (
                  <div style={{ marginTop: 12, color: '#ff4d4f' }}>
                    <p style={{ margin: 0, fontSize: 12 }}>
                      ⚠️ 系统提示：管理后台尚未配置全局对象存储，请先前往「<Link to={`/${adminPath}/settings?tab=database&subtab=storage`}>站点设置 → 存储设置</Link>」中完成对象存储配置。
                    </p>
                  </div>
                )}
              </div>
            }
            style={{
              background: storageConfig?.global_configured ? 'rgba(82,196,26,0.06)' : 'rgba(250,173,20,0.06)',
              border: storageConfig?.global_configured ? '1px solid rgba(82,196,26,0.2)' : '1px solid rgba(250,173,20,0.2)',
              padding: '16px'
            }}
          />
        </div>
      )}
    </div>
  );

  const handleToggleReviewEnabled = async (checked: boolean) => {
    setReviewEnabled(checked);
    try {
      const values = moderationForm.getFieldsValue();
      await request.post(`/plugins/${name}/moderation-config`, { ...values, review_enabled: checked });
      message.success(checked ? '素材审核功能已开启' : '素材审核功能已关闭');
    } catch (error: any) {
      console.error(error);
      setReviewEnabled(!checked);
    }
  };

  // ====== 审核配置 Tab ======
  const moderationTab = (
    <div>
      {/* 审核功能开关 */}
      <div style={{
        background: _isLight ? '#fff' : '#141414', borderRadius: 8,
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        padding: '16px 20px', marginBottom: 16,
        display: 'flex', alignItems: 'center', justifyContent: 'space-between'
      }}>
        <div>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>素材审核功能</Text><br />
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
            {moderationConfig?.is_configured
              ? '开启后用户上传的素材需要通过火山引擎审核才能使用；关闭后素材上传即可用，无需审核流程。'
              : '请先完成下方私域虚拟人像素材资产库配置（Access Key / Secret Key）后才可开启审核功能。'}
          </Text>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Tag color={reviewEnabled ? 'success' : 'default'} style={{ margin: 0, background: reviewEnabled ? 'rgba(82,196,26,0.1)' : 'rgba(255,255,255,0.04)', border: 'none' }}>{reviewEnabled ? '已开启' : '已关闭'}</Tag>
          <Switch
            checked={reviewEnabled}
            disabled={!moderationConfig?.is_configured}
            onChange={handleToggleReviewEnabled}
          />
        </div>
      </div>

      {/* 状态提示 */}
      {moderationConfig && (
        <div style={{ marginBottom: 16 }}>
          {moderationConfig.is_configured ? (
            <Alert
              type="success"
              showIcon
              icon={<CheckCircleOutlined />}
              message="私域素材资产库已配置"
              description="Access Key 已配置，可正常使用虚拟人像上传与审核功能"
              style={{ background: 'rgba(82,196,26,0.06)', border: '1px solid rgba(82,196,26,0.2)' }}
            />
          ) : (
            <Alert
              type="warning"
              showIcon
              message="私域素材资产库未配置"
              description="虚拟人像功能需要配置火山引擎 API 访问密钥，请前往火山引擎控制台获取"
              style={{ background: 'rgba(250,173,20,0.06)', border: '1px solid rgba(250,173,20,0.2)' }}
            />
          )}
        </div>
      )}

      <div style={{
        background: _isLight ? '#fff' : '#141414', borderRadius: 8,
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
          <ApiOutlined style={{ color: '#1677ff', fontSize: 16 }} />
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>私域虚拟人像素材资产库配置</Text>
        </div>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginBottom: 16 }}>
          请前往火山引擎控制台 → 头像下拉菜单 →「API访问密钥」页面，创建并获取 Access Key ID 和 Secret Access Key
        </Text>

        <Form form={moderationForm} layout="vertical" requiredMark={false}>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0 16px' }}>
            <Form.Item label={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>Access Key ID</Text>} name="volc_access_key" rules={[{ required: true, message: '请输入 Access Key ID' }]}>
              <Input placeholder="Access Key ID" style={inputStyle} />
            </Form.Item>
            <Form.Item
              label={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>Secret Access Key</Text>}
              name="volc_secret_key"
              rules={[{ required: !moderationConfig?.is_configured, message: '请输入 Secret Access Key' }]}
              extra={moderationConfig?.volc_secret_key_masked ? <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 11 }}>当前: {moderationConfig.volc_secret_key_masked}（留空则不修改）</Text> : undefined}
            >
              <Input.Password placeholder="Secret Access Key" style={inputStyle} />
            </Form.Item>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '0 16px' }}>
            <Form.Item
              label={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>使用项目名称 (ProjectName)</Text>}
              name="volc_project_name"
              extra={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 11 }}>接口调用时使用的项目名称，留空则默认为 default</Text>}
            >
              <Input placeholder="default" style={inputStyle} />
            </Form.Item>
            <Form.Item
              label={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>API 地域 (Region)</Text>}
              name="volc_region"
              extra={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 11 }}>选择火山引擎方舟 API 的服务地域，影响签名和请求路由</Text>}
            >
              <Select
                placeholder="选择 API 地域"
                style={{ width: '100%' }}
                popupClassName="dark-select-dropdown"
                options={[
                  { value: 'cn-beijing', label: <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}><span>🇨🇳 国内版</span><span style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>cn-beijing</span></div> },
                  { value: 'ap-southeast-1', label: <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}><span>🌏 国际版</span><span style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>ap-southeast-1</span></div> },
                ]}
              />
            </Form.Item>
          </div>
          {(name === 'asset_manager' || name === 'asset_manager_intl') && (
            <Form.Item
              label={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>Ark 转换素材组 ID (GroupID)</Text>}
              name="volc_group_id"
              extra={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)', fontSize: 11 }}>系统将在首次转换素材时自动向方舟申请并绑定专属群组 ID，自动管理无需干预</Text>}
            >
              <Input disabled placeholder="留空交由系统自动为您生成管理" style={{ ...inputStyle, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }} />
            </Form.Item>
          )}
        </Form>

        {/* 审核请求地址信息 */}
        {moderationConfig?.is_configured && (
          <div style={{
            marginTop: 16, padding: '12px 16px', borderRadius: 6,
            background: _isLight ? 'rgba(22,119,255,0.04)' : 'rgba(22,119,255,0.08)',
            border: _isLight ? '1px solid rgba(22,119,255,0.15)' : '1px solid rgba(22,119,255,0.2)'
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
              <CloudServerOutlined style={{ color: '#1677ff', fontSize: 13 }} />
              <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13, fontWeight: 500 }}>审核请求地址</Text>
              <Tag color="blue" style={{ margin: 0, fontSize: 11, lineHeight: '18px' }}>
                {(moderationConfig?.volc_region || 'cn-beijing') === 'ap-southeast-1' ? '国际版' : '国内版'}
              </Tag>
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 11, whiteSpace: 'nowrap' }}>WithRegion</Text>
                <Text copyable code style={{ fontSize: 12, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                  {moderationConfig?.volc_region || 'cn-beijing'}
                </Text>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 11, whiteSpace: 'nowrap' }}>Endpoint</Text>
                <Text copyable code style={{ fontSize: 12, fontFamily: 'monospace', wordBreak: 'break-all' }}>
                  {moderationConfig?.review_api_url || 'https://open.volcengineapi.com/?Action=CreateAsset&Version=2024-01-01'}
                </Text>
              </div>
            </div>
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
          <Button type="primary" icon={<SaveOutlined />} loading={savingModeration} onClick={handleSaveModeration}>保存审核配置</Button>
        </div>
      </div>
    </div>
  );

  // ====== 高可用配置 Tab：列表 + 创建/编辑弹窗 ======
  const haConfigTab = (
    <div>
      <div style={{
        background: _isLight ? '#fff' : '#141414', borderRadius: 8,
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '16px 20px'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 14, flexWrap: 'wrap' }}>
          <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>高可用规则模板</Text>
          <Space size={8} wrap>
            <Popconfirm
              title="清除本进程运行时缓存？"
              description="清空熔断表和规则缓存，下次调用从库加载。不改绑定与规则。"
              onConfirm={handleClearHaRuntime}
              okText="清除"
              cancelText="取消"
            >
              <Button icon={<ReloadOutlined />} loading={clearingHaRuntime}>
                清除运行时缓存
              </Button>
            </Popconfirm>
            <Button type="primary" icon={<PlusOutlined />} disabled={haRules.length >= HA_MAX_RULES} onClick={openHaCreate}>
              新建规则
            </Button>
          </Space>
        </div>
        <Table
          rowKey="id"
          size="middle"
          pagination={false}
          dataSource={haRules}
          scroll={{ x: 900 }}
          locale={{ emptyText: '暂无规则' }}
          columns={[
            {
              title: '规则名称',
              dataIndex: 'name',
              ellipsis: true,
              render: (name: string, r: HaRuleDto) => (
                <Space size={8}>
                  <Text strong>{name}</Text>
                  {r.id === haDef && <Tag color="blue">默认</Tag>}
                </Space>
              ),
            },
            {
              title: '最大尝试',
              dataIndex: 'retries',
              width: 90,
              align: 'center',
            },
            {
              title: '首包超时',
              dataIndex: 'ttfb',
              width: 90,
              align: 'center',
              render: (v?: number) => ((v && v > 0) ? `${v}s` : '关闭'),
            },
            {
              title: '墙钟预算',
              dataIndex: 'budget',
              width: 100,
              render: (v: number) => `${v > 0 ? v : 900}s`,
            },
            {
              title: '终态错误',
              dataIndex: 'err',
              width: 90,
              render: (v: string) => (v === 'last' ? '末败' : '首败'),
            },
            {
              title: '熔断摘要',
              key: 'melt',
              ellipsis: true,
              render: (_: unknown, r: HaRuleDto) => (
                <Text type="secondary" style={{ fontSize: 12 }}>{haMeltSummary(r.melt)}</Text>
              ),
            },
            {
              title: '名单',
              key: 'lists',
              width: 120,
              render: (_: unknown, r: HaRuleDto) => (
                <Text type="secondary" style={{ fontSize: 12 }}>
                  白{(r.allow || []).length} / 黑{(r.deny || []).length}
                </Text>
              ),
            },
            {
              title: '操作',
              key: 'actions',
              width: 268,
              fixed: 'right' as const,
              render: (_: unknown, r: HaRuleDto) => (
                <Space size={4} wrap>
                  <Button type="link" size="small" icon={<EditOutlined />} onClick={() => openHaEdit(r)}>编辑</Button>
                  <Button
                    type="link"
                    size="small"
                    icon={<CopyOutlined />}
                    disabled={haRules.length >= HA_MAX_RULES}
                    onClick={() => openHaCopy(r)}
                  >
                    复制
                  </Button>
                  <Button type="link" size="small" disabled={r.id === haDef} onClick={() => setHaDefault(r.id)}>设为默认</Button>
                  <Popconfirm
                    title="确认删除该规则？"
                    disabled={haRules.length <= 1 || r.id === haDef}
                    onConfirm={() => deleteHaRule(r.id)}
                  >
                    <Button
                      type="link"
                      size="small"
                      danger
                      icon={<DeleteOutlined />}
                      disabled={haRules.length <= 1 || r.id === haDef}
                    >
                      删除
                    </Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      </div>

      <Modal
        title={haDraftIsNew ? '新建高可用规则' : '编辑高可用规则'}
        open={!!haDraft}
        onCancel={() => setHaDraft(null)}
        onOk={commitHaDraft}
        confirmLoading={savingHa}
        okText="确定"
        cancelText="取消"
        width={720}
        destroyOnHidden
        styles={{ body: { maxHeight: '70vh', overflowY: 'auto', paddingTop: 12 } }}
      >
        {haDraft && (
          <>
            <Row gutter={16}>
              <Col span={24}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12, display: 'block', marginBottom: 6 }}>规则名称</Text>
                  <Input
                    value={haDraft.name}
                    maxLength={64}
                    onChange={e => patchHaDraft(r => ({ ...r, name: e.target.value }))}
                  />
                </div>
              </Col>
            </Row>
            <Row gutter={16}>
              <Col span={12}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>最大尝试次数（含首次）</Text>
                  <InputNumber
                    min={1}
                    max={HA_MAX_RETRIES}
                    style={{ width: '100%' }}
                    value={haDraft.retries}
                    onChange={v => patchHaDraft(r => ({ ...r, retries: Number(v) || 1 }))}
                  />
                  <Text type="secondary" style={{ fontSize: 11, display: 'block', marginTop: 4 }}>
                    含第一次调用；未达上限也可能因无可用子渠、熔断或黑名单提前停
                  </Text>
                </div>
              </Col>
              <Col span={12}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>终态错误</Text>
                  <div>
                    <Radio.Group
                      value={haDraft.err || 'first'}
                      onChange={e => patchHaDraft(r => ({ ...r, err: e.target.value }))}
                    >
                      <Radio.Button value="first">首败</Radio.Button>
                      <Radio.Button value="last">末败</Radio.Button>
                    </Radio.Group>
                  </div>
                  <Text type="secondary" style={{ fontSize: 11, display: 'block', marginTop: 4 }}>
                    首败对外展示首次失败原因；末败展示最后一次尝试的错误
                  </Text>
                </div>
              </Col>
            </Row>
            <Row gutter={16}>
              <Col span={12}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>单次首包超时（秒，0=关闭）</Text>
                  <InputNumber
                    min={0}
                    max={3600}
                    style={{ width: '100%' }}
                    value={haDraft.ttfb ?? 0}
                    onChange={v => patchHaDraft(r => ({ ...r, ttfb: v === null ? 0 : Math.max(0, Number(v)) }))}
                  />
                  <Text type="secondary" style={{ fontSize: 11, display: 'block', marginTop: 4 }}>
                    单次超时上限（0=关闭）。生图专线建议设为 0 或 120s+ 避免误掐；超时切备渠防假死
                  </Text>
                </div>
              </Col>
              <Col span={12}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>整次墙钟预算（秒）</Text>
                  <InputNumber
                    min={10}
                    max={3600}
                    style={{ width: '100%' }}
                    value={haDraft.budget > 0 ? haDraft.budget : 900}
                    onChange={v => patchHaDraft(r => ({ ...r, budget: Math.max(10, Number(v) || 900) }))}
                  />
                  <Text type="secondary" style={{ fontSize: 11, display: 'block', marginTop: 4 }}>
                    整组调度的最终时间兜底底线（默认900s），需避开外层反向代理 504
                  </Text>
                </div>
              </Col>
            </Row>
            <Text strong style={{ display: 'block', marginBottom: 8 }}>熔断冷却（秒，0=关闭）</Text>
            <Row gutter={16}>
              {HA_MELT_CODES.filter(c => c !== '401' && c !== '402').map(code => (
                <Col span={12} key={code}>
                  <div style={{ marginBottom: 16 }}>
                    <Text type="secondary" style={{ fontSize: 12 }}>{haMeltLabel(code)}</Text>
                    <InputNumber
                      min={0}
                      style={{ width: '100%' }}
                      value={haDraft.melt?.[code] || 0}
                      onChange={v => setHaDraftMelt(code, Number(v) || 0)}
                    />
                  </div>
                </Col>
              ))}
              <Col span={12}>
                <div style={{ marginBottom: 16 }}>
                  <Text type="secondary" style={{ fontSize: 12 }}>401·402 鉴权失效/欠费</Text>
                  <InputNumber
                    min={0}
                    style={{ width: '100%' }}
                    value={haDraft.melt?.['401'] || haDraft.melt?.['402'] || 0}
                    onChange={v => setHaDraftAuthMelt(Number(v) || 0)}
                  />
                </div>
              </Col>
            </Row>
            <div style={{ marginTop: 8 }}>
              <Text type="secondary" style={{ fontSize: 12 }}>不熔断白名单（报错关键词）</Text>
              <Select
                mode="tags"
                style={{ width: '100%', marginBottom: 16 }}
                placeholder="回车添加"
                tokenSeparators={['\n']}
                open={false}
                suffixIcon={null}
                value={haDraft.allow || []}
                onChange={(v: string[]) => patchHaDraft(r => ({ ...r, allow: v }))}
              />
              <Text type="secondary" style={{ fontSize: 12 }}>停止切换黑名单（报错关键词）</Text>
              <Select
                mode="tags"
                style={{ width: '100%' }}
                placeholder="回车添加"
                tokenSeparators={['\n']}
                open={false}
                suffixIcon={null}
                value={haDraft.deny || []}
                onChange={(v: string[]) => patchHaDraft(r => ({ ...r, deny: v }))}
              />
            </div>
          </>
        )}
      </Modal>

      <Modal
        title="复制高可用规则"
        open={!!haCopySource}
        onCancel={() => setHaCopySource(null)}
        onOk={commitHaCopy}
        confirmLoading={savingHa}
        okText="确定"
        cancelText="取消"
        destroyOnHidden
      >
        <div style={{ marginBottom: 8 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            将复制「{haCopySource?.name}」的全部参数，仅名称不同
          </Text>
        </div>
        <Input
          autoFocus
          maxLength={64}
          placeholder="请输入新规则名称"
          value={haCopyName}
          onChange={e => setHaCopyName(e.target.value)}
          onPressEnter={commitHaCopy}
        />
      </Modal>
    </div>
  );

  // ====== 审核日志 Tab ======


  const fetchAuditLogs = async () => {
    try {
      setAuditLoading(true);
      const res = await (request.get(`/assets/admin/list?category=虚拟人像`, { headers: { 'x-plugin-ns': name } }) as any);
      if (res.assets) {
        setAuditLogs(res.assets);
      }
      if (res.uid_map) {
        setAuditUidMap(res.uid_map);
      }
    } catch (e) {
      console.error('获取审核日志失败', e);
    } finally {
      setAuditLoading(false);
    }
  };

  const auditLogColumns = [
    {
      title: '用户 UID',
      dataIndex: 'user_id',
      key: 'user_id',
      width: 140,
      render: (userId: string) => {
        const info = auditUidMap[userId];
        return info ? (
          <span>
            <Text copyable style={{ fontSize: 12 }}>{info.uid}</Text>
            <div style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)' }}>{info.username}</div>
          </span>
        ) : <Text style={{ fontSize: 12 }}>{userId?.slice(0, 8)}...</Text>;
      }
    },
    {
      title: '文件名',
      dataIndex: 'file_name',
      key: 'file_name',
      ellipsis: true
    },
    {
      title: '预览',
      key: 'preview',
      width: 80,
      render: (_: any, record: any) => {
        if (record.file_url) {
          let url = record.file_url;
          if (!url.startsWith('http') && !url.startsWith('/')) url = `https://${url}`;
          return <img src={url} alt="" style={{ width: 50, height: 50, objectFit: 'cover', borderRadius: 4 }} />;
        }
        return <Text type="secondary">-</Text>;
      }
    },
    {
      title: 'Asset ID',
      dataIndex: 'asset_id',
      key: 'asset_id',
      width: 160,
      render: (aid: string) => aid ? <Text code style={{ fontSize: 11 }}>{aid.slice(0, 20)}...</Text> : <Text type="secondary">暂无</Text>
    },
    {
      title: '状态',
      dataIndex: 'status',
      key: 'status',
      width: 120,
      render: (status: string, record: any) => {
        if (status === 'uploaded') return <Tag color="blue" icon={<SendOutlined />}>待提交审核</Tag>;
        if (status === 'processing') return <Tag color="processing" icon={<LoadingOutlined spin />}>审核中</Tag>;
        if (status === 'approved') return <Tag color="success" icon={<CheckCircleOutlined />}>已通过</Tag>;
        if (status === 'rejected') return (
          <span>
            <Tag color="error" icon={<CloseCircleOutlined />}>已驳回</Tag>
            {record.reject_reason && <div style={{ fontSize: 11, color: '#ff4d4f', marginTop: 2 }}>{record.reject_reason}</div>}
          </span>
        );
        if (status === 'pending') return <Tag color="warning">待审核</Tag>;
        return <Tag>{status}</Tag>;
      }
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 185,
      render: (t: string) => t ? <Text style={{ fontSize: 12 }}>{formatApiDateTime(t)}</Text> : '-'
    },
  ];


  const fetchAssetInfo = async (assetId: string) => {
    if (!assetId || expandedAssetInfo[assetId]) return;
    try {
      setLoadingAssetInfo(prev => ({ ...prev, [assetId]: true }));
      const res = await (request.get(`/assets/admin/get-asset-info/${assetId}`, { headers: { 'x-plugin-ns': name } }) as any);
      setExpandedAssetInfo(prev => ({ ...prev, [assetId]: res }));
    } catch (e: any) {
      setExpandedAssetInfo(prev => ({ ...prev, [assetId]: { error: e?.response?.data?.error?.message || '查询失败' } }));
    } finally {
      setLoadingAssetInfo(prev => ({ ...prev, [assetId]: false }));
    }
  };

  const auditLogTab = (
    <div>
      <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 13 }}>共 {auditLogs.length} 条虚拟人像上传记录</Text>
        <Button size="small" onClick={fetchAuditLogs} loading={auditLoading}>刷新</Button>
      </div>
      <Table
        dataSource={auditLogs}
        columns={auditLogColumns}
        rowKey="id"
        loading={auditLoading}
        size="small"
        scroll={{ x: 860 }}
        pagination={listPagination({
          current: auditPage,
          pageSize: auditPageSize,
          onChange: (page, size) => {
            setAuditPage(page);
            setAuditPageSize(size);
          },
        })}
        expandable={{
          expandedRowRender: record => {
            const aid = record.asset_id;
            if (!aid) return <Text type="secondary">该素材尚未提交到火山引擎，无详细信息</Text>;
            const info = expandedAssetInfo[aid];
            const loading = loadingAssetInfo[aid];
            if (loading || !info) return <Spin size="small" />;
            if (info?.error) return <Text type="danger">查询失败: {info.error}</Text>;
            if (!info) return <Spin size="small" />;
            return (
              <div style={{ padding: 16, background: _isLight ? '#fafafa' : '#1a1a1a', borderRadius: 8 }}>
                <Text strong style={{ color: '#1677ff', display: 'block', marginBottom: 12, fontSize: 14 }}>📄 火山引擎素材详情 (GetAsset)</Text>
                <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '10px 24px' }}>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>Asset ID</Text><br /><Text copyable style={{ fontSize: 13, fontFamily: 'monospace' }}>{info.Id}</Text></div>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>Group ID</Text><br /><Text copyable style={{ fontSize: 13, fontFamily: 'monospace' }}>{info.GroupId}</Text></div>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>状态 (Status)</Text><br /><Tag color={info.Status === 'Active' ? 'success' : info.Status === 'Failed' ? 'error' : 'processing'}>{info.Status}</Tag></div>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>素材类型 (AssetType)</Text><br /><Text style={{ fontSize: 13 }}>{info.AssetType}</Text></div>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>项目名称 (ProjectName)</Text><br /><Text style={{ fontSize: 13 }}>{info.ProjectName}</Text></div>
                  <div><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>创建时间 (CreateTime)</Text><br /><Text style={{ fontSize: 13 }}>{info.CreateTime ? formatApiDateTime(info.CreateTime) : '-'}</Text></div>
                  <div style={{ gridColumn: '1 / -1' }}><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>URL</Text><br /><Text copyable style={{ fontSize: 12, fontFamily: 'monospace', wordBreak: 'break-all' }}>{info.URL}</Text></div>
                </div>
              </div>
            );
          },
          onExpand: (expanded, record) => {
            if (expanded && record.asset_id) fetchAssetInfo(record.asset_id);
          }
        }}
      />
    </div>
  );


  const SOURCE_MAP: Record<string, { label: string; color: string }> = {
    api_proxy: { label: 'API 接口调用', color: 'blue' },
    page: { label: '页面操作', color: 'green' },
    relay_convert: { label: '转发规则替换', color: 'orange' }
  };

  const apiLogColumns = [
    { title: 'ID', dataIndex: 'id', key: 'id', width: 60 },
    {
      title: 'User ID',
      dataIndex: 'user_id',
      key: 'user_id',
      width: 140,
      render: (userId: string) => {
        const info = auditUidMap[userId];
        return info ? (
          <span>
            <Text copyable style={{ fontSize: 12 }}>{info.uid}</Text>
            <div style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)' }}>{info.username}</div>
          </span>
        ) : <Text style={{ fontSize: 12 }}>{userId?.slice(0, 8)}...</Text>;
      }
    },
    { title: '接口名称', dataIndex: 'api_endpoint', key: 'api_endpoint', width: 180, render: (r: string) => <Tag color="cyan">{r}</Tag> },
    {
      title: '来源',
      dataIndex: 'source',
      key: 'source',
      width: 120,
      render: (s: string) => {
        const info = SOURCE_MAP[s] || { label: s, color: 'default' };
        return <Tag color={info.color}>{info.label}</Tag>;
      }
    },
    {
      title: '状态',
      dataIndex: 'status_code',
      key: 'status_code',
      width: 100,
      render: (s: number) => {
        if (s === 200) return <Tag color="success">成功 ({s})</Tag>;
        return <Tag color="error">失败 ({s})</Tag>;
      }
    },
    { title: '请求时间', dataIndex: 'created_at', key: 'created_at', width: 185, render: (t: string) => <Text style={{ fontSize: 12 }}>{formatApiDateTime(t)}</Text> },
  ];

  const apiLogTab = (
    <div>
      <PluginLogRetentionCard pluginName={name || ''} title="接口日志保留天数" />
      <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Select
            placeholder="来源筛选" allowClear
            value={logSourceFilter || undefined}
            onChange={(val) => { setLogSourceFilter(val || ''); setTimeout(() => fetchApiLogs(1), 0); }}
            style={{ width: 150 }}
            options={[
              { value: 'api_proxy', label: 'API 接口调用' },
              { value: 'page', label: '页面操作' },
              { value: 'relay_convert', label: '转发规则替换' },
            ]}
          />
          <Select
            placeholder="按状态筛选" allowClear
            value={logStatusFilter || undefined}
            onChange={(val) => {
              const next = val || '';
              setLogStatusFilter(next);
              fetchApiLogs(1, apiLogsPageSize, next);
            }}
            style={{ width: 120 }}
            options={[
              { value: 'success', label: '成功' },
              { value: 'failed', label: '失败' },
            ]}
          />
          <Input.Search
            placeholder="搜索接口名 / 用户UID"
            allowClear
            value={logKeyword}
            onChange={(e) => {
              const val = e.target.value;
              setLogKeyword(val);
              if (!val) {
                fetchApiLogs(1, apiLogsPageSize, logStatusFilter);
              }
            }}
            onSearch={() => fetchApiLogs(1)}
            style={{ width: 220 }}
          />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 13 }}>共 {apiLogsTotal} 条记录</Text>
          <Button size="small" onClick={() => fetchApiLogs(apiLogsPage)} loading={apiLogsLoading}>刷新</Button>
        </div>
      </div>
      <Table
        dataSource={apiLogs}
        columns={apiLogColumns}
        rowKey="id"
        loading={apiLogsLoading}
        size="small"
        scroll={{ x: 880 }}
        pagination={listPagination({
          current: apiLogsPage,
          total: apiLogsTotal,
          pageSize: apiLogsPageSize,
          onChange: (page, size) => {
            setApiLogsPageSize(size);
            fetchApiLogs(page, size);
          },
        })}
        expandable={{
          expandedRowRender: (record) => (
            <ApiLogPayloadExpand
              request={record.request_payload}
              response={record.response_payload}
            />
          ),
        }}
      />
    </div>
  );



  const filteredPgModels = pgModels.filter(m => {
    if (pgSelectedProvider != null && m.provider_id !== pgSelectedProvider) return false;
    if (pgSelectedApiProvider != null && m.api_provider_id !== pgSelectedApiProvider) return false;
    if (pgSelectedType != null && m.type_id !== pgSelectedType) return false;
    if (pgEnabledFilter === 'enabled' && !m.pg_enabled) return false;
    if (pgEnabledFilter === 'disabled' && !!m.pg_enabled) return false;
    return modelMatchesKeyword(m, pgSearchKeyword);
  });

  const pgModelColumns = [
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_model_name', '模型名称') : '模型名称',
      dataIndex: 'name',
      key: 'name',
      width: 260,
      render: (name: string, record: any) => {
        return (
          <div style={{ maxWidth: 240, display: 'flex', flexDirection: 'column', gap: 2 }}>
            <div style={{ lineHeight: 1.2 }}>
              <span
                style={{
                  fontWeight: 600,
                  color: _isLight ? '#1f2937' : '#fff',
                  fontSize: 13,
                  lineHeight: 1.2,
                  wordBreak: 'break-all',
                  display: 'inline-block'
                }}
                title={name}
              >
                {name}
              </span>
            </div>
            {record.remark && (
              <div
                style={{
                  fontSize: 12,
                  lineHeight: 1.2,
                  color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }}
                title={record.remark}
              >
                {record.remark}
              </div>
            )}
            <div style={{ fontSize: 11, lineHeight: 1.2, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={`MID: ${record.mid} | ${record.model_id}`}>
              MID: {record.mid} | {record.model_id}
            </div>
          </div>
        );
      }
    },
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_type', '类型') : '类型',
      dataIndex: 'type_name',
      key: 'type_name',
      width: 100,
      render: (t: string) => t ? (
        <Tag style={{ borderRadius: 4, background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.04)', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>
          {t.includes('封装') ? <ThunderboltOutlined style={{ marginRight: 4 }} /> : t.includes('视频') ? <VideoCameraOutlined style={{ marginRight: 4 }} /> : t.includes('图片') ? <PictureOutlined style={{ marginRight: 4 }} /> : (name === 'playground_2026' && t.includes('音频')) ? <AudioOutlined style={{ marginRight: 4 }} /> : null}
          {t}
        </Tag>
      ) : <Text type="secondary">-</Text>
    },
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_enable_creation', '启用创作') : '启用创作',
      key: 'pg_enabled',
      width: 90,
      align: 'center' as const,
      render: (_: any, record: any) => (
        <Switch
          checked={record.pg_enabled}
          onChange={(val) => handlePgToggle(record.id, val)}
        />
      )
    },
    ...(name === 'playground_2026' ? [
      {
        title: (
          <Tooltip title={pg2026T('admin_col_mp_enabled_tip', '与「模型广场管理 → 模型列表 → 模型广场展示」双向同步：开关开启时选择不可查看的用户等级（含未登录游客，默认全部可查看）；开关关闭时选择可查看的用户等级（默认全部不可查看）。不支持展示的用户等级即使开启创作，在创作中心用户端也不会展示该模型。')}>
            <span>{pg2026T('admin_col_mp_enabled', '模型广场展示')}</span>
          </Tooltip>
        ),
        key: 'mp_level_ids',
        width: 240,
        render: (_: any, record: any) => (
          <MpLevelSelectCell
            record={record}
            levels={levels}
            _isLight={_isLight}
            onToggle={handleMpToggle}
            onChange={handleMpLevelIdsChange}
          />
        )
      },
    ] : []),
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_bound_scheme', '绑定方案') : '绑定方案',
      key: 'pg_scheme_id',
      width: name === 'playground_2026' ? 220 : 180,
      render: (_: any, record: any) => {
        const scheme = pgSchemes.find(s => s.id === record.pg_scheme_id);
        if (!scheme) {
          return (
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 12 }}>{pg2026T('admin_unbound', '未绑定')}</Text>
          );
        }
        return (
          <div style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
            <Tag color="blue" style={{ borderRadius: 12, fontSize: 12, marginInlineEnd: 0 }}>{scheme.name}</Tag>
            {name === 'playground_2026' && (
              <Popconfirm
                title={pg2026T('admin_unbind_confirm', '确定取消绑定该方案？')}
                okText={pg2026T('admin_scheme_delete', '删除')}
                cancelText={pg2026T('admin_cancel', '取消')}
                onConfirm={() => handleUnbindScheme(record.id)}
              >
                <Button
                  type="text"
                  size="small"
                  danger
                  icon={<DeleteOutlined />}
                  disabled={savingPlayground}
                  style={{ padding: '0 4px', height: 22, minWidth: 22 }}
                  title={pg2026T('admin_unbind_btn', '删除绑定')}
                />
              </Popconfirm>
            )}
          </div>
        );
      }
    },
    {
      title: (
        <Tooltip title={name === 'playground_2026' ? pg2026T('admin_col_page_sort_tip', '与模型列表「页面排序」同步，数值越大越靠前') : '与模型列表「页面排序」同步，数值越大越靠前'}>
          <span>{name === 'playground_2026' ? pg2026T('admin_col_page_sort', '页面排序') : '页面排序'}</span>
        </Tooltip>
      ),
      key: 'pg_sort_order',
      width: 120,
      sorter: (a: any, b: any) => (a.pg_sort_order || 0) - (b.pg_sort_order || 0),
      render: (_: any, record: any) => {
        const isEditing = editingSortModelId === record.id;
        if (isEditing) {
          return (
            <div style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
              <InputNumber
                size="small"
                min={0}
                max={9999}
                autoFocus
                value={editingSortValue}
                onChange={(val) => setEditingSortValue(val ?? 0)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    handleSaveInlineSort(record.id);
                  } else if (e.key === 'Escape') {
                    setEditingSortModelId(null);
                  }
                }}
                style={{ width: 66 }}
              />
              <Button
                type="text"
                size="small"
                icon={<CheckOutlined style={{ color: '#52c41a', fontSize: 12 }} />}
                onClick={() => handleSaveInlineSort(record.id)}
                style={{ padding: '0 4px', height: 22, minWidth: 22 }}
                title={name === 'playground_2026' ? pg2026T('admin_save', '保存') : '保存'}
              />
              <Button
                type="text"
                size="small"
                icon={<CloseOutlined style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12 }} />}
                onClick={() => setEditingSortModelId(null)}
                style={{ padding: '0 4px', height: 22, minWidth: 22 }}
                title={name === 'playground_2026' ? pg2026T('admin_cancel', '取消') : '取消'}
              />
            </div>
          );
        }
        return (
          <div style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 13, fontFamily: 'monospace', minWidth: 16 }}>
              {record.pg_sort_order || 0}
            </span>
            <Button
              type="text"
              size="small"
              icon={<EditOutlined style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }} />}
              onClick={() => {
                setEditingSortModelId(record.id);
                setEditingSortValue(record.pg_sort_order || 0);
              }}
              style={{ padding: '0 4px', height: 22, minWidth: 22 }}
              title={name === 'playground_2026' ? pg2026T('admin_sort_edit_tip', '修改页面排序') : '修改页面排序'}
            />
          </div>
        );
      }
    },
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_default_display', '默认展示') : '默认展示',
      key: 'pg_default',
      width: 100,
      align: 'center' as const,
      filters: [
        { text: name === 'playground_2026' ? pg2026T('admin_filter_default_set', '已设默认') : '已设默认', value: 'default' },
        { text: name === 'playground_2026' ? pg2026T('admin_filter_default_unset', '未设默认') : '未设默认', value: 'not_default' },
      ],
      filterMultiple: false,
      onFilter: (value: any, record: any) => {
        const isDefault = Array.isArray(pgDefaultModelMids) && pgDefaultModelMids.includes(record.mid);
        if (value === 'default') return isDefault;
        if (value === 'not_default') return !isDefault;
        return true;
      },
      render: (_: any, record: any) => {
        const isChecked = Array.isArray(pgDefaultModelMids) && pgDefaultModelMids.includes(record.mid);
        return (
          <Switch
            size="small"
            checked={isChecked}
            onChange={async (checked) => {
              const prevList = Array.isArray(pgDefaultModelMids) ? pgDefaultModelMids : [];
              const nextList = checked
                ? (prevList.includes(record.mid) ? prevList : [...prevList, record.mid])
                : prevList.filter(mid => mid !== record.mid);
              setPgDefaultModelMids(nextList);
              if (name === 'playground_2026') {
                const ok = await persistPlaygroundModels(pgModels, { defaultModelMids: nextList, successMessage: checked ? pg2026T('admin_default_set_toast', '已设为默认展示') : pg2026T('admin_default_unset_toast', '已取消默认展示') });
                if (!ok) setPgDefaultModelMids(prevList);
              }
            }}
            disabled={!record.pg_enabled || (name === 'playground_2026' && savingPlayground)}
          />
        );
      }
    },
    {
      title: name === 'playground_2026' ? pg2026T('admin_col_action', '操作') : '操作',
      key: 'action',
      width: name === 'playground_2026' ? 340 : 200,
      render: (_: any, record: any) => {
        const featureKind = name === 'playground_2026' ? featureKindFromTypeName(record.type_name) : null;
        const schemeBtn = (
          <Button
            type="text"
            size="small"
            icon={<SettingOutlined />}
            onClick={() => handleOpenSchemeDrawer(record.id, record.pg_scheme_id)}
            style={{ color: '#1677ff', padding: '0 4px' }}
          >
            {name === 'playground_2026' ? pg2026T('admin_act_scheme', '方案') : '方案'}
          </Button>
        );
        return (
        <div style={{ display: 'flex', gap: 4, flexWrap: 'nowrap', alignItems: 'center' }}>
          {name !== 'playground_2026' && schemeBtn}
          {record.pg_scheme_id && (
            <>
              <Button
                type="text"
                size="small"
                icon={<EditOutlined />}
                onClick={() => {
                  setPgOverrideModelId(record.id);
                  setPgOverrideData(record.pg_param_overrides || { modify: {}, remove: [], add: [] });
                  if (isPg2026ImageModel(record)) {
                    setPgIspDraft(initPgIspDraft(record));
                  } else {
                    setPgIspDraft(null);
                  }
                  const po = record.pg_prompt_optimize;
                  setPgPoDraft(parsePromptOptimizeBinding(po));
                  const vl = record.pg_voice_library;
                  setPgVlDraft(vl && typeof vl === 'object' ? parseVoiceLibraryBinding(vl) : null);
                  setPgIspModalVisible(false);
                  setPgOverrideModalVisible(true);
                }}
                style={{ color: '#faad14', padding: '0 4px' }}
              >
                {name === 'playground_2026' ? pg2026T('admin_act_tune', '调参') : '调参'}
              </Button>
              <Button
                type="text"
                size="small"
                icon={<ApiOutlined />}
                onClick={() => {
                  setPgIoOverrideModelId(record.id);
                  setPgIoOverrideData(record.pg_io_overrides || null);
                  setPgIoOverrideModalVisible(true);
                }}
                style={{ color: '#13c2c2', padding: '0 4px' }}
              >
                {name === 'playground_2026' ? pg2026T('admin_act_io', 'IO配置') : 'IO配置'}
              </Button>
            </>
          )}
          {name === 'playground_2026' && record.pg_enabled && featureKind && (
            <Button
              type="text"
              size="small"
              icon={<TagsOutlined />}
              onClick={() => {
                setPgFeatureModelId(record.id);
                setPgFeatureDraft(
                  featureKind === 'image'
                    ? normalizeImageFeatureAttrs(parseFeatureAttrList(record.feature_attributes))
                    : parseFeatureAttrList(record.feature_attributes),
                );
                setPgInputProtocolDraft(typeof record.pg_input_protocol === 'string' ? record.pg_input_protocol : '');
                setPgThinkingProfileDraft(typeof record.pg_thinking_profile === 'string' ? record.pg_thinking_profile : '');
                setPgFeatureModalVisible(true);
              }}
              style={{ color: '#722ed1', padding: '0 4px' }}
            >
              {name === 'playground_2026' ? pg2026T('admin_act_feature', '特性配置') : '特性配置'}
            </Button>
          )}
          {name === 'playground_2026' && schemeBtn}
        </div>
        );
      }
    },
  ];

  const playgroundModelTab = (
    <div>
      <div style={{ background: _isLight ? '#fff' : '#141414', borderRadius: 8, padding: '20px', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', marginBottom: 16 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_models_title', '可创作模型列表')}</Text>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, display: 'block', marginTop: 4 }}>
              {name === 'playground_2026'
                ? pg2026T('admin_models_desc', '开启启用开关并绑定方案后即可使用。方案、调参、IO、特性点击确认后立即生效；图片模型可在调参中开关专用参数。选中「默认展示」可将模型设为默认（支持多选）。')
                : '开启启用开关并绑定方案后，用户即可在模型创作中心使用该模型。选中"默认展示"列后，可将模型设为默认展示（支持多选）。'}
            </Text>
          </div>
          <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
            <Radio.Group value={pgEnabledFilter} onChange={e => setPgEnabledFilter(e.target.value)}>
              <Radio.Button value="all">{pg2026T('admin_filter_all', '全部')}</Radio.Button>
              <Radio.Button value="enabled">{pg2026T('admin_filter_enabled', '已开启')}</Radio.Button>
              <Radio.Button value="disabled">{pg2026T('admin_filter_disabled', '未开启')}</Radio.Button>
            </Radio.Group>
            <Input
              placeholder={pg2026T('admin_search_models', '搜索模型...')}
              value={pgSearchKeyword}
              onChange={e => setPgSearchKeyword(e.target.value)}
              style={{ width: 220 }}
              allowClear
              prefix={<SearchOutlined style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)' }} />}
            />
          </div>
        </div>

        <div style={{ marginBottom: 16 }}>
          <ClassificationFilter
            providers={pgProvidersStats}
            apiProviders={pgApiProvidersStats}
            types={pgTypesStats}
            selectedProvider={pgSelectedProvider}
            selectedApiProvider={pgSelectedApiProvider}
            selectedType={pgSelectedType}
            onProviderChange={setPgSelectedProvider}
            onApiProviderChange={setPgSelectedApiProvider}
            onTypeChange={(id) => {
              setPgSelectedType(id);
            }}
          />
        </div>

        <Table
          dataSource={filteredPgModels}
          columns={pgModelColumns}
          rowKey="id"
          size="small"
          scroll={{ x: name === 'playground_2026' ? 1580 : 1000 }}
          pagination={listPagination({
            current: pgPage,
            pageSize: pgPageSize,
            onChange: onPgPageChange,
          })}
          style={{ marginBottom: name === 'playground_2026' ? 0 : 16 }}
        />

        {name !== 'playground_2026' && (
          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Button type="primary" loading={savingPlayground} onClick={handleSavePlaygroundConfig} icon={<SaveOutlined />}>
              保存全部配置
            </Button>
          </div>
        )}
      </div>

      {/* 参数覆写 Modal */}
      <Modal
        title={
          name === 'playground_2026' ? (
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', paddingRight: 28, gap: 12 }}>
              <span style={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {pg2026T('admin_override_title', `参数调整 — ${pgModels.find(m => m.id === pgOverrideModelId)?.name || ''}`, { name: pgModels.find(m => m.id === pgOverrideModelId)?.name || '' })}
              </span>
              <Button
                size="small"
                icon={<ReloadOutlined />}
                loading={savingPlayground}
                onClick={handleResetParamOverrides}
              >
                {pg2026T('admin_override_reset', '重置')}
              </Button>
            </div>
          ) : `参数调整 — ${pgModels.find(m => m.id === pgOverrideModelId)?.name || ''}`
        }
        open={pgOverrideModalVisible}
        onCancel={() => {
          setPgOverrideModalVisible(false);
          setPgIspModalVisible(false);
        }}
        width={name === 'playground_2026' ? 760 : 640}
        footer={
          name === 'playground_2026' ? (
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
              <Button onClick={() => { setPgOverrideModalVisible(false); setPgIspModalVisible(false); }} disabled={savingPlayground}>{pg2026T('admin_cancel', '取消')}</Button>
              <Button type="primary" loading={savingPlayground} onClick={handleConfirmParamOverrides}>{pg2026T('admin_confirm', '确认')}</Button>
            </div>
          ) : (
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <Button danger loading={savingPlayground} onClick={handleResetParamOverrides}>重置为预设</Button>
              <div style={{ display: 'flex', gap: 8 }}>
                <Button onClick={() => { setPgOverrideModalVisible(false); setPgIspModalVisible(false); }} disabled={savingPlayground}>取消</Button>
                <Button type="primary" loading={savingPlayground} onClick={handleConfirmParamOverrides}>确认</Button>
              </div>
            </div>
          )
        }
      >
        {(() => {
          const model = pgModels.find(m => m.id === pgOverrideModelId);
          const scheme = pgSchemes.find(s => s.id === model?.pg_scheme_id);
          if (!scheme) return <Text type="secondary">{name === 'playground_2026' ? pg2026T('admin_override_no_scheme', '该模型未绑定方案') : '该模型未绑定方案'}</Text>;
          if (!scheme?.params && !(name === 'playground_2026' && (scheme.type === 'image' || scheme.type === 'video' || scheme.type === 'audio'))) {
            return <Text type="secondary">{name === 'playground_2026' ? pg2026T('admin_override_no_params', '该模型未绑定方案或方案无参数') : '该模型未绑定方案或方案无参数'}</Text>;
          }
          const overrides = pgOverrideData || { modify: {}, remove: [], add: [] };
          const removes = new Set(overrides.remove || []);
          const modifies = overrides.modify || {};
          const showQuickBar = name === 'playground_2026' && (scheme.type === 'image' || scheme.type === 'video' || scheme.type === 'audio');
          const tryEnableQuick = (merged: any, skip: { key?: string; addIndex?: number }) => {
            if (!isQuickBarEligible(merged)) {
              message.error(name === 'playground_2026' ? pg2026T('admin_override_quick_bar_ineligible', '快捷栏仅支持已配置选项的单选或下拉参数') : '快捷栏仅支持已配置选项的单选或下拉参数');
              return false;
            }
            if (countQuickBarEnabled(scheme.params || [], overrides, skip) >= SCHEME_QUICK_BAR_MAX) {
              message.error(name === 'playground_2026' ? pg2026T('admin_override_quick_bar_tip', `快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`, { max: SCHEME_QUICK_BAR_MAX }) : `快捷栏最多开启 ${SCHEME_QUICK_BAR_MAX} 个参数`);
              return false;
            }
            return true;
          };

          return (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 12 }}>
                {name === 'playground_2026'
                  ? pg2026T('admin_override_notice', `基于系统方案「${scheme.name}」的模型级调参，控件类型、选项与默认值以此面板为准（仅对本模型生效）`, { name: scheme.name })
                  : `基于方案「${scheme.name}」的参数个性化调整（仅对此模型生效）`}
              </Text>
              {name === 'playground_2026' && (scheme.type === 'image' || scheme.type === 'video') && (
                <>
                  <SchemePromptOptimizeField
                    mode="model"
                    schemeType={scheme.type}
                    value={pgPoDraft}
                    inheritValue={scheme.prompt_optimize}
                    inheritSchemeName={scheme.name}
                    isLight={_isLight}
                    onChange={setPgPoDraft}
                  />
                  <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
                </>
              )}
              {name === 'playground_2026' && scheme.type === 'audio' && (
                <>
                  <SchemeVoiceLibraryField
                    mode="model"
                    schemeType={scheme.type}
                    value={pgVlDraft}
                    inheritValue={scheme.voice_library}
                    inheritSchemeName={scheme.name}
                    catalogs={pgVoiceCatalogs}
                    isLight={_isLight}
                    onChange={setPgVlDraft}
                  />
                  <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
                </>
              )}
              {name === 'playground_2026' && scheme.type === 'image' && (
                <>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 }}>
                    <div style={{ minWidth: 0 }}>
                      <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_isp_title', '图片专用参数配置')}</Text>
                      <Text style={{ display: 'block', fontSize: 12, marginTop: 2, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>
                        {pg2026T('admin_isp_desc_model', '对本模型生效。开启后，图片生成页属性选择器展示比例、尺寸与分辨率专用控件')}
                      </Text>
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                      {!!pgIspDraft?.enabled && (
                        <Button
                          size="small"
                          icon={<SettingOutlined />}
                          onClick={() => {
                            const next = seedImageSpecialParams(pgIspDraft);
                            setPgIspDraft(next);
                            setPgIspBaseline(JSON.parse(JSON.stringify(next)));
                            setPgIspModalVisible(true);
                          }}
                        >
                          {pg2026T('admin_isp_btn_config', '配置')}
                        </Button>
                      )}
                      <Tooltip
                        title={isImageSpecialForcedOn(scheme?.id) ? pg2026T('admin_isp_tip_forced', 'GPT-Image-2 按官方规则必须开启图片专用参数') : undefined}
                      >
                        <Switch
                          checked={isImageSpecialForcedOn(scheme?.id) || !!pgIspDraft?.enabled}
                          disabled={isImageSpecialForcedOn(scheme?.id)}
                          onChange={(v) => {
                            if (isImageSpecialForcedOn(scheme?.id)) return;
                            if (!v) setPgIspModalVisible(false);
                            setPgIspDraft(v
                              ? seedImageSpecialParams(pgIspDraft)
                              : { ...normalizeImageSpecialParams(pgIspDraft), enabled: false });
                          }}
                        />
                      </Tooltip>
                    </div>
                  </div>
                  <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
                </>
              )}
              {/* 预设参数列表 */}
              { (scheme.params || []).map((p: any) => {
                const isRemoved = removes.has(p.key);
                const mod = modifies[p.key] || {};
                const mergedParam = { ...p, ...mod };
                if (
                  name === 'playground_2026'
                  && (isSingleCountParam(mergedParam) || (isSeedream50ProModel(model) && isCountParam(mergedParam)))
                ) {
                  return null;
                }
                const schemeParamOn = p.enabled !== false;
                const isParamOn = name === 'playground_2026'
                  ? !isRemoved && (mod.enabled ?? schemeParamOn)
                  : !isRemoved;
                const effectiveType = mod.type || p.type;
                const fieldHint = { fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 4 } as const;
                const sameValue = (a: any, b: any) => {
                  if (Array.isArray(a) && Array.isArray(b)) return JSON.stringify(a) === JSON.stringify(b);
                  if (a && typeof a === 'object' && b && typeof b === 'object') {
                    return JSON.stringify(a) === JSON.stringify(b);
                  }
                  return a === b || String(a) === String(b);
                };
                const applyModPatch = (partial: Record<string, any>) => {
                  const next = { ...mod };
                  Object.entries(partial).forEach(([field, value]) => {
                    if (sameValue(value, p[field])) delete next[field];
                    else next[field] = value;
                  });
                  const newMod = { ...modifies };
                  if (Object.keys(next).length === 0) delete newMod[p.key];
                  else newMod[p.key] = next;
                  setPgOverrideData({ ...overrides, modify: newMod });
                };
                const patchModField = (field: string, value: any) => {
                  applyModPatch({ [field]: value });
                };
                const parseNum = (raw: string, fallback: number) => {
                  const n = Number(raw);
                  return Number.isFinite(n) ? n : fallback;
                };
                return (
                  <div key={p.key} style={{
                    padding: '10px 14px', borderRadius: 8,
                    background: isRemoved || !isParamOn ? (_isLight ? 'rgba(255,0,0,0.03)' : 'rgba(255,0,0,0.06)') : (_isLight ? '#fafafa' : '#1a1a1a'),
                    border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
                    opacity: isParamOn ? 1 : 0.5,
                  }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: isParamOn ? 8 : 0 }}>
                      <div>
                        <Text strong style={{ fontSize: 13, color: _isLight ? '#1f2937' : '#fff' }}>{isPg2026 ? formatParamLabel(mergedParam) : p.label}</Text>
                        <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', marginLeft: 8 }}>{p.key}</Text>
                      </div>
                      <Switch
                        checked={isParamOn}
                        onChange={(checked) => {
                          if (name === 'playground_2026' && p.enabled === false) {
                            const newRemoves = (overrides.remove || []).filter((k: string) => k !== p.key);
                            const next = { ...mod };
                            if (checked) next.enabled = true;
                            else delete next.enabled;
                            const newMod = { ...modifies };
                            if (Object.keys(next).length === 0) delete newMod[p.key];
                            else newMod[p.key] = next;
                            setPgOverrideData({ ...overrides, remove: newRemoves, modify: newMod });
                            return;
                          }
                          const newRemoves = checked ? (overrides.remove || []).filter((k: string) => k !== p.key) : [...(overrides.remove || []), p.key];
                          setPgOverrideData({ ...overrides, remove: newRemoves });
                        }}
                        size="small"
                      />
                    </div>
                    {isParamOn && (
                      name === 'playground_2026' ? (
                      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                        <div style={{
                          display: 'grid',
                          gridTemplateColumns: effectiveType === 'slider'
                            ? 'minmax(148px, 1.3fr) repeat(4, minmax(0, 1fr))'
                            : 'minmax(160px, 200px) minmax(120px, 160px)',
                          gap: 8,
                          alignItems: 'end',
                        }}>
                          <div>
                            <Text style={fieldHint}>{pg2026T('admin_override_field_control', '控件类型')}</Text>
                            <Select
                              size="small"
                              value={effectiveType}
                              style={{ width: '100%' }}
                              options={SCHEME_CONTROL_TYPE_OPTIONS.map(opt => ({ ...opt, label: pg2026T(`admin_ctrl_${opt.value}`, opt.label) }))}
                              onChange={(v) => applyModPatch(patchForControlTypeChange(p, mod, v))}
                            />
                          </div>
                          {effectiveType === 'slider' && ([
                            { field: 'min', label: `${pg2026T('admin_override_field_min', '最小值')}${p.unit ? ` (${p.unit})` : ''}`, fallback: 0 },
                            { field: 'max', label: `${pg2026T('admin_override_field_max', '最大值')}${p.unit ? ` (${p.unit})` : ''}`, fallback: 100 },
                            { field: 'step', label: pg2026T('admin_override_field_step', '步长'), fallback: 1 },
                          ] as const).map(({ field, label, fallback }) => {
                            const original = p[field] ?? fallback;
                            return (
                              <div key={field}>
                                <Text style={fieldHint}>{label}</Text>
                                <Input
                                  size="small"
                                  type="number"
                                  value={String(mod[field] !== undefined ? mod[field] : original)}
                                  onChange={e => patchModField(field, parseNum(e.target.value, original))}
                                />
                              </div>
                            );
                          })}
                          <div>
                            <Text style={fieldHint}>{pg2026T('admin_override_field_default', '默认值')}</Text>
                            {effectiveType === 'switch' ? (
                              <Switch
                                size="small"
                                checked={mod.default !== undefined ? mod.default : p.default}
                                onChange={v => patchModField('default', v)}
                              />
                            ) : (
                              <Input
                                size="small"
                                value={String(mod.default !== undefined ? mod.default : p.default ?? '')}
                                onChange={e => {
                                  const rawVal = e.target.value;
                                  let val: any = rawVal;
                                  if (typeof (mod.default ?? p.default) === 'number' || effectiveType === 'slider') {
                                    if (rawVal !== '' && !rawVal.endsWith('.') && !isNaN(Number(rawVal))) {
                                      val = Number(rawVal);
                                    } else if (effectiveType === 'slider') {
                                      val = Number(rawVal) || 0;
                                    }
                                  }
                                  patchModField('default', val);
                                }}
                              />
                            )}
                          </div>
                        </div>
                        {effectiveType !== 'switch' && effectiveType !== 'slider' && (
                          <>
                            <div>
                              <Text style={fieldHint}>{pg2026T('admin_override_field_options', '选项列表')}</Text>
                              <Input
                                size="small"
                                value={Array.isArray(mergedParam.options) ? mergedParam.options.join(', ') : ''}
                                placeholder={pg2026T('admin_override_field_options_ph', '用英文逗号分隔，例如 Low,Medium,High')}
                                onChange={(e) => {
                                  const { options, option_labels: parsedLabels } = parseOptionsAndLabels(e.target.value);
                                  const srcOpts = p.options || [];
                                  const isNumeric = typeof (mod.default ?? p.default) === 'number'
                                    || (Array.isArray(srcOpts) && srcOpts.length > 0 && typeof srcOpts[0] === 'number');
                                  const nextOpts = isNumeric
                                    ? options.map((x) => (!Number.isNaN(Number(x)) ? Number(x) : x))
                                    : options;
                                  const mergedLabels = { ...(mergedParam.option_labels || {}), ...parsedLabels };
                                  applyModPatch({
                                    options: nextOpts,
                                    option_labels: Object.keys(mergedLabels).length > 0 ? mergedLabels : {},
                                  });
                                }}
                              />
                            </div>
                            <div>
                              <Text style={fieldHint}>{pg2026T('admin_override_field_mapping', '选项中文映射')}</Text>
                              <Input
                                size="small"
                                value={formatOptionLabelsDisplay(mergedParam.option_labels, mergedParam.options)}
                                placeholder={pg2026T('admin_override_field_mapping_ph', '可选，例如 Low:常用画质,Medium:高画质,High:高精细画质')}
                                onChange={(e) => {
                                  const newLabels = parseOptionLabelsMapping(e.target.value, mergedParam.options || []);
                                  applyModPatch({
                                    option_labels: Object.keys(newLabels).length > 0 ? newLabels : {},
                                  });
                                }}
                              />
                              {renderPgOptionPreview(mergedParam)}
                            </div>
                          </>
                        )}
                        {showQuickBar && (
                          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                              <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>{pg2026T('admin_override_quick_bar', '快捷栏')}</Text>
                              <Tooltip title={pg2026T('admin_quick_bar_help', SCHEME_QUICK_BAR_HELP)}>
                                <QuestionCircleOutlined style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', cursor: 'help' }} />
                              </Tooltip>
                            </div>
                            <Switch
                              size="small"
                              checked={!!(mod.quick !== undefined ? mod.quick : p.quick)}
                              disabled={!isQuickBarEligible({ ...p, ...mod }) && !(mod.quick !== undefined ? mod.quick : p.quick)}
                              onChange={(v) => {
                                if (v && !tryEnableQuick({ ...p, ...mod }, { key: p.key })) return;
                                patchModField('quick', v);
                              }}
                            />
                          </div>
                        )}
                      </div>
                      ) : (
                      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                        {effectiveType !== 'switch' && effectiveType !== 'slider' && Array.isArray(p.options) && (
                          <div style={{ flex: 1, minWidth: 200 }}>
                            <Text style={fieldHint}>选项 (逗号分隔)</Text>
                            <Input
                              size="small"
                              defaultValue={(mod.options || p.options || []).join(', ')}
                              key={`opts-${p.key}-${effectiveType}-${JSON.stringify(mod.options || p.options || [])}`}
                              onBlur={e => {
                                const srcOpts = mod.options || p.options || [];
                                const isNumeric = typeof (mod.default ?? p.default) === 'number' || (Array.isArray(srcOpts) && srcOpts.length > 0 && typeof srcOpts[0] === 'number') || effectiveType === 'slider';
                                const newOpts = e.target.value.split(',').map((s: string) => s.trim()).filter(Boolean).map(x => (isNumeric && !isNaN(Number(x))) ? Number(x) : x);
                                patchModField('options', newOpts);
                              }}
                              onPressEnter={e => (e.target as HTMLInputElement).blur()}
                            />
                          </div>
                        )}
                        {effectiveType === 'slider' && ([
                          { field: 'min', label: `最小值${p.unit ? ` (${p.unit})` : ''}`, fallback: 0, width: 100 },
                          { field: 'max', label: `最大值${p.unit ? ` (${p.unit})` : ''}`, fallback: 100, width: 100 },
                          { field: 'step', label: '步长', fallback: 1, width: 80 },
                        ] as const).map(({ field, label, fallback, width }) => {
                          const original = p[field] ?? fallback;
                          return (
                            <div key={field} style={{ width }}>
                              <Text style={fieldHint}>{label}</Text>
                              <Input
                                size="small"
                                type="number"
                                value={String(mod[field] !== undefined ? mod[field] : original)}
                                onChange={e => patchModField(field, parseNum(e.target.value, original))}
                              />
                            </div>
                          );
                        })}
                        <div style={{ width: 120 }}>
                          <Text style={fieldHint}>默认值</Text>
                          {effectiveType === 'switch' ? (
                            <Switch
                              size="small"
                              checked={mod.default !== undefined ? mod.default : p.default}
                              onChange={v => patchModField('default', v)}
                            />
                          ) : (
                            <Input
                              size="small"
                              value={String(mod.default !== undefined ? mod.default : p.default ?? '')}
                              onChange={e => {
                                const rawVal = e.target.value;
                                let val: any = rawVal;
                                if (typeof (mod.default ?? p.default) === 'number' || effectiveType === 'slider') {
                                  if (rawVal !== '' && !rawVal.endsWith('.') && !isNaN(Number(rawVal))) {
                                    val = Number(rawVal);
                                  } else if (effectiveType === 'slider') {
                                    val = Number(rawVal) || 0;
                                  }
                                }
                                patchModField('default', val);
                              }}
                            />
                          )}
                        </div>
                      </div>
                      )
                    )}
                  </div>
                );
              })}
              {/* 新增参数 */}
              <Divider style={{ margin: '4px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <Text style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{pg2026T('admin_custom_param', '自定义新增参数')}</Text>
                <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={() => {
                  const adds = [...(overrides.add || [])];
                  adds.push({ key: '', label: '', type: 'select', options: [], default: '' });
                  setPgOverrideData({ ...overrides, add: adds });
                }}>{pg2026T('admin_scheme_add_param', '添加')}</Button>
              </div>
              {(overrides.add || []).map((ap: any, idx: number) => {
                const updateAdd = (patch: any) => {
                  const adds = [...overrides.add];
                  adds[idx] = { ...ap, ...patch };
                  setPgOverrideData({ ...overrides, add: adds });
                };
                return (
                  <div key={idx} style={{ padding: '10px 12px', borderRadius: 6, background: _isLight ? 'rgba(22,119,255,0.03)' : 'rgba(22,119,255,0.06)', border: '1px solid rgba(22,119,255,0.15)', marginTop: 8 }}>
                    <div style={{ display: 'flex', gap: 8, alignItems: 'flex-end', marginBottom: 8 }}>
                      <div style={{ flex: 1 }}>
                        <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>Key</Text>
                        <Input size="small" value={ap.key} onChange={e => updateAdd({ key: e.target.value })} />
                      </div>
                      <div style={{ flex: 1 }}>
                        <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_label', '标签')}</Text>
                        <Input size="small" value={ap.label} onChange={e => updateAdd({ label: e.target.value })} />
                      </div>
                      <div style={{ width: 100 }}>
                        <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_control', '类型')}</Text>
                        <Select size="small" value={ap.type} onChange={v => updateAdd({ type: v, options: (v === 'select' || v === 'radio') ? (ap.options || []) : undefined, min: v === 'slider' ? (ap.min ?? 0) : undefined, max: v === 'slider' ? (ap.max ?? 1) : undefined, step: v === 'slider' ? (ap.step ?? 0.1) : undefined, quick: (v === 'select' || v === 'radio') ? ap.quick : false })} style={{ width: '100%' }}
                          options={[{ label: 'Select', value: 'select' }, { label: 'Radio', value: 'radio' }, { label: 'Switch', value: 'switch' }, { label: 'Slider', value: 'slider' }]} />
                      </div>
                      <Button type="text" size="small" icon={<DeleteOutlined />} danger onClick={() => {
                        const adds = overrides.add.filter((_: any, i: number) => i !== idx);
                        setPgOverrideData({ ...overrides, add: adds });
                      }} />
                    </div>
                    {/* 按类型显示对应配置字段 */}
                    {(ap.type === 'select' || ap.type === 'radio') && (
                      <>
                      <div style={{ display: 'flex', gap: 8, marginBottom: 6 }}>
                        <div style={{ flex: 1 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>
                            {name === 'playground_2026' ? pg2026T('admin_override_field_options', '选项列表') : '选项 (逗号分隔)'}
                          </Text>
                          {name === 'playground_2026' ? (
                            <Input
                              size="small"
                              value={Array.isArray(ap.options) ? ap.options.join(',') : ''}
                              placeholder={pg2026T('admin_override_field_options_ph', '例如: opaque,transparent 或 16:9,9:16')}
                              onChange={(e) => {
                                const { options, option_labels: parsedLabels } = parseOptionsAndLabels(e.target.value);
                                const isNumeric = options.length > 0 && options.every((x) => !Number.isNaN(Number(x)));
                                const parsedOpts = isNumeric ? options.map(Number) : options;
                                const mergedLabels = { ...(ap.option_labels || {}), ...parsedLabels };
                                updateAdd({
                                  options: parsedOpts,
                                  option_labels: Object.keys(mergedLabels).length > 0 ? mergedLabels : undefined,
                                });
                              }}
                            />
                          ) : (
                          <Input size="small" defaultValue={(ap.options || []).join(', ')} key={`add-opts-${idx}-${(ap.options || []).length}`}
                            onBlur={e => {
                              const opts = e.target.value.split(',').map((s: string) => s.trim()).filter(Boolean);
                              const isNumeric = opts.length > 0 && opts.every(x => !isNaN(Number(x)));
                              const parsedOpts = isNumeric ? opts.map(Number) : opts;
                              updateAdd({ options: parsedOpts });
                            }}
                            onPressEnter={e => (e.target as HTMLInputElement).blur()} />
                          )}
                        </div>
                        <div style={{ width: 100 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_default', '默认值')}</Text>
                          <Input size="small" value={String(ap.default || '')} onChange={e => {
                            const rawVal = e.target.value;
                            let val: any = rawVal;
                            const isNumericOptions = Array.isArray(ap.options) && ap.options.length > 0 && ap.options.every((x: any) => typeof x === 'number');
                            if ((isNumericOptions || !isNaN(Number(rawVal))) && rawVal !== '' && !rawVal.endsWith('.')) {
                              val = Number(rawVal);
                            }
                            updateAdd({ default: val });
                          }} />
                        </div>
                      </div>
                      {name === 'playground_2026' && (
                        <div style={{ marginBottom: 6 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>
                            {pg2026T('admin_override_field_mapping', '选项中文映射')}
                          </Text>
                          <Input
                            size="small"
                            value={formatOptionLabelsDisplay(ap.option_labels, ap.options)}
                            placeholder={pg2026T('admin_override_field_mapping_ph', '可选，例如 opaque:不透明,transparent:透明')}
                            onChange={(e) => {
                              const newLabels = parseOptionLabelsMapping(e.target.value, ap.options || []);
                              updateAdd({ option_labels: Object.keys(newLabels).length > 0 ? newLabels : undefined });
                            }}
                          />
                          {renderPgOptionPreview(ap)}
                        </div>
                      )}
                      </>
                    )}
                    {ap.type === 'slider' && (
                      <div style={{ display: 'flex', gap: 8, marginBottom: 6 }}>
                        <div style={{ flex: 1 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_min', '最小值')}</Text>
                          <Input size="small" type="number" value={ap.min ?? 0} onChange={e => updateAdd({ min: Number(e.target.value) })} />
                        </div>
                        <div style={{ flex: 1 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_max', '最大值')}</Text>
                          <Input size="small" type="number" value={ap.max ?? 1} onChange={e => updateAdd({ max: Number(e.target.value) })} />
                        </div>
                        <div style={{ flex: 1 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_step', '步长')}</Text>
                          <Input size="small" type="number" value={ap.step ?? 0.1} onChange={e => updateAdd({ step: Number(e.target.value) })} />
                        </div>
                        <div style={{ flex: 1 }}>
                          <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_default', '默认值')}</Text>
                          <Input size="small" type="number" value={ap.default ?? 0} onChange={e => updateAdd({ default: Number(e.target.value) })} />
                        </div>
                      </div>
                    )}
                    {ap.type === 'switch' && (
                      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 6 }}>
                        <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_default', '默认值')}</Text>
                        <Switch size="small" checked={!!ap.default} onChange={v => updateAdd({ default: v })} />
                      </div>
                    )}
                    <div>
                      <Text style={{ fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', display: 'block', marginBottom: 2 }}>{pg2026T('admin_override_field_unit_ph', '提示 (选填)')}</Text>
                      <Input size="small" value={ap.hint || ''} onChange={e => updateAdd({ hint: e.target.value || undefined })} placeholder={pg2026T('admin_param_hint_placeholder', '参数说明')} />
                    </div>
                    {showQuickBar && (
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 8 }}>
                        <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>{pg2026T('admin_override_quick_bar', '快捷栏')}</Text>
                        <Tooltip title={pg2026T('admin_quick_bar_help', SCHEME_QUICK_BAR_HELP)}>
                          <QuestionCircleOutlined style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', cursor: 'help' }} />
                        </Tooltip>
                        <Switch
                          size="small"
                          checked={!!ap.quick}
                          disabled={!isQuickBarEligible(ap) && !ap.quick}
                          onChange={(v) => {
                            if (v && !tryEnableQuick(ap, { addIndex: idx })) return;
                            updateAdd({ quick: v });
                          }}
                        />
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          );
        })()}
      </Modal>

      {/* IO 覆写 Modal */}
      <Modal
        title={name === 'playground_2026' ? `${pg2026T('admin_act_io', 'IO配置')} — ${pgModels.find(m => m.id === pgIoOverrideModelId)?.name || ''}` : `IO配置 — ${pgModels.find(m => m.id === pgIoOverrideModelId)?.name || ''}`}
        open={pgIoOverrideModalVisible}
        onCancel={() => setPgIoOverrideModalVisible(false)}
        width={780}
        style={{ top: 40 }}
        bodyStyle={{ maxHeight: 'calc(80vh - 120px)', overflowY: 'auto' }}
        footer={
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <Button danger loading={savingPlayground} onClick={handleResetIoOverrides}>{name === 'playground_2026' ? pg2026T('admin_reset_preset', '重置为方案默认') : '重置为方案默认'}</Button>
            <div style={{ display: 'flex', gap: 8 }}>
              <Button onClick={() => setPgIoOverrideModalVisible(false)} disabled={savingPlayground}>{name === 'playground_2026' ? pg2026T('admin_cancel', '取消') : '取消'}</Button>
              <Button type="primary" loading={savingPlayground} onClick={handleConfirmIoOverrides}>{name === 'playground_2026' ? pg2026T('admin_confirm', '确认') : '确认'}</Button>
            </div>
          </div>
        }
      >
        {(() => {
          const model = pgModels.find(m => m.id === pgIoOverrideModelId);
          const scheme = pgSchemes.find(s => s.id === model?.pg_scheme_id);
          if (!scheme) return <Text type="secondary">{name === 'playground_2026' ? pg2026T('admin_override_no_scheme', '该模型未绑定方案') : '该模型未绑定方案'}</Text>;
          const seeded = ensureSchemeIoDefaults(scheme);
          return (
            <div>
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 12, display: 'block', marginBottom: 12 }}>
                {name === 'playground_2026' ? pg2026T('admin_scheme_io_model_notice', `基于方案「${scheme.name}」个性化调整（仅对此模型生效），页面与方案 IO 配置一致`, { name: scheme.name }) : `基于方案「${scheme.name}」个性化调整（仅对此模型生效），页面与方案 IO 配置一致`}
              </Text>
              <ModelIoOverridesEditor
                schemeId={scheme.id}
                schemeType={scheme.type}
                schemeParams={scheme.params || []}
                schemeInputs={seeded.inputs || []}
                schemeOutputs={seeded.outputs || []}
                overrides={pgIoOverrideData}
                onChange={setPgIoOverrideData}
                isLight={_isLight}
                localizeLabels={isPg2026}
              />
            </div>
          );
        })()}
      </Modal>

      {name === 'playground_2026' && (
        <Modal
          title={pg2026T('admin_feature_title', `特性配置 — ${pgModels.find(m => m.id === pgFeatureModelId)?.name || ''}`, { name: pgModels.find(m => m.id === pgFeatureModelId)?.name || '' })}
          open={pgFeatureModalVisible}
          onCancel={() => { if (!pgFeatureSaving) setPgFeatureModalVisible(false); }}
          confirmLoading={pgFeatureSaving}
          width={560}
          onOk={async () => {
            const model = pgModels.find(m => m.id === pgFeatureModelId);
            if (!model) {
              setPgFeatureModalVisible(false);
              return;
            }
            const attrs = parseFeatureAttrList(pgFeatureDraft);
            const isChat = featureKindFromTypeName(model.type_name) === 'chat';
            const next = pgModels.map(m => m.id === pgFeatureModelId ? {
              ...m,
              feature_attributes: attrs,
              ...(isChat ? {
                pg_input_protocol: pgInputProtocolDraft || undefined,
                pg_thinking_profile: pgThinkingProfileDraft || undefined,
              } : {}),
            } : m);
            setPgFeatureSaving(true);
            const ok = await persistPlaygroundModels(next, { successMessage: pg2026T('admin_feature_saved', '特性配置已保存') });
            setPgFeatureSaving(false);
            if (!ok) return;
            setPgModels(next);
            setPgFeatureModalVisible(false);
          }}
          okText={pg2026T('admin_confirm', '确认')}
          cancelText={pg2026T('admin_cancel', '取消')}
        >
          {(() => {
            const model = pgModels.find(m => m.id === pgFeatureModelId);
            if (!model || !featureKindFromTypeName(model.type_name)) {
              return <Text type="secondary">{pg2026T('admin_feature_no_options', '该模型类型未配置二级功能属性')}</Text>;
            }
            const fromType = pgModelTypes.find((t) => t.id === model.type_id)?.default_features;
            const kind = featureKindFromTypeName(model.type_name);
            let options = parseFeatureAttrList(fromType?.length ? fromType : model.type_default_features);
            if (kind === 'image') options = imageFeatureOptions(options);
            if (kind === 'chat') {
              for (const attr of CHAT_UNDERSTAND_ATTRS) {
                if (!options.includes(attr)) options = [...options, attr];
              }
            }
            return (
              <div>
                <FeatureAttributesEditor
                  value={pgFeatureDraft}
                  options={options}
                  onChange={setPgFeatureDraft}
                  isLight={_isLight}
                />
                {kind === 'chat' ? (
                  <div style={{ marginTop: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
                    <div>
                      <Text style={{ display: 'block', marginBottom: 6, fontSize: 12 }}>{pg2026T('admin_input_protocol', '输入协议')}</Text>
                      <Select
                        value={pgInputProtocolDraft || undefined}
                        placeholder={pg2026T('admin_input_protocol_ph', '未设置（沿用内置规则）')}
                        style={{ width: '100%' }}
                        optionLabelProp="label"
                        popupMatchSelectWidth
                        options={CHAT_INPUT_PROTOCOLS.map((p) => ({
                          value: p.id,
                          label: chatProtocolSelectLabel(p.label, p.detail, _isLight),
                        }))}
                        onChange={(v) => setPgInputProtocolDraft(typeof v === 'string' ? v : '')}
                      />
                    </div>
                    <div>
                      <Text style={{ display: 'block', marginBottom: 6, fontSize: 12 }}>{pg2026T('admin_thinking_profile', '思考预设')}</Text>
                      <Select
                        value={pgThinkingProfileDraft || undefined}
                        placeholder={pg2026T('admin_thinking_profile_ph', '未设置（沿用内置规则）')}
                        style={{ width: '100%' }}
                        optionLabelProp="label"
                        popupMatchSelectWidth
                        options={CHAT_THINKING_PROFILES.map((p) => ({
                          value: p.id,
                          label: chatProtocolSelectLabel(p.label, p.detail, _isLight),
                        }))}
                        onChange={(v) => setPgThinkingProfileDraft(typeof v === 'string' ? v : '')}
                      />
                    </div>
                  </div>
                ) : null}
              </div>
            );
          })()}
        </Modal>
      )}

      {name === 'playground_2026' && (
        <Modal
          title={pg2026T('admin_isp_title', '图片专用参数配置')}
          open={pgIspModalVisible}
          onCancel={() => {
            setPgIspDraft(pgIspBaseline);
            setPgIspModalVisible(false);
          }}
          width={640}
          zIndex={1100}
          footer={
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
              <Button
                onClick={() => {
                  const model = pgModels.find(m => m.id === pgOverrideModelId);
                  const scheme = pgSchemes.find((s: any) => s.id === model?.pg_scheme_id);
                  setPgIspDraft(officialImageSpecialParams(scheme?.id));
                }}
              >
                {pg2026T('admin_reset', '重置')}
              </Button>
              <Button
                onClick={() => {
                  setPgIspDraft(pgIspBaseline);
                  setPgIspModalVisible(false);
                }}
              >
                {pg2026T('admin_cancel', '取消')}
              </Button>
              <Button type="primary" onClick={() => setPgIspModalVisible(false)}>{pg2026T('admin_done', '完成')}</Button>
            </div>
          }
        >
          {(() => {
            const model = pgModels.find(m => m.id === pgOverrideModelId);
            const scheme = pgSchemes.find((s: any) => s.id === model?.pg_scheme_id);
            const ov = pgOverrideData || { modify: {}, remove: [], add: [] };
            const removes = new Set(ov.remove || []);
            const modifies = ov.modify || {};
            const mergedParams = (scheme?.params || [])
              .filter((p: any) => !removes.has(p.key))
              .map((p: any) => ({ ...p, ...(modifies[p.key] || {}) }))
              .concat(ov.add || []);
            return (
              <ImageSpecialParamsEditor
                value={pgIspDraft}
                params={mergedParams}
                onChange={(next) => setPgIspDraft({ ...next, enabled: true })}
                isLight={_isLight}
              />
            );
          })()}
        </Modal>
      )}

      {/* 方案选择 Drawer */}
      <Drawer
        title={pg2026T('admin_drawer_bind_scheme', '选择创作方案')}
        open={pgSchemeDrawerVisible}
        onClose={() => setPgSchemeDrawerVisible(false)}
        width={580}
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button loading={savingPlayground} onClick={() => handleConfirmScheme('')}>{pg2026T('admin_btn_unbind', '取消绑定')}</Button>
            <Button type="primary" loading={savingPlayground} onClick={() => handleConfirmScheme()}>{pg2026T('admin_btn_bind_confirm', '确认绑定')}</Button>
          </div>
        }
      >
        <div style={{ marginBottom: 16 }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 13 }}>
            {pg2026T('admin_drawer_bind_scheme_to', '为模型 {{name}} 选择一个创作方案', { name: pgModels.find(m => m.id === pgCurrentId)?.name || '' })}
          </Text>
        </div>
        {(() => {
          const currentModelType = pgModels.find(m => m.id === pgCurrentId)?.type_name || '';
          const modelSchemeType = currentModelType.includes('视频')
            ? 'video'
            : currentModelType.includes('图片')
              ? 'image'
              : name === 'playground_2026' && (currentModelType.includes('音频') || currentModelType.includes('语音'))
                ? 'audio'
                : 'chat';
          const filteredSchemes = pgSchemes.filter(s => s.type === modelSchemeType);
          return (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {filteredSchemes.map(scheme => {
                const isSelected = pgSelectedSchemeId === scheme.id;
                return (
                  <div
                    key={scheme.id}
                    style={{
                      padding: '10px 14px', borderRadius: 6,
                      border: isSelected ? '1px solid #1677ff' : (_isLight ? '1px solid #e4e4e7' : '1px solid #27272a'),
                      background: isSelected ? (_isLight ? 'rgba(22,119,255,0.05)' : 'rgba(22,119,255,0.12)') : (_isLight ? '#fafafa' : '#18181b'),
                      cursor: 'pointer', transition: 'all 0.15s',
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      gap: 12
                    }}
                    onClick={() => setPgSelectedSchemeId(scheme.id)}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 0, flexWrap: 'wrap' }}>
                      <Text strong style={{ color: isSelected ? '#1677ff' : (_isLight ? '#09090b' : '#f4f4f5'), fontSize: 13 }}>
                        {scheme.name}
                      </Text>
                      {scheme.is_system && (
                        <Tag
                          style={{
                            fontSize: 10,
                            borderRadius: 4,
                            lineHeight: '16px',
                            padding: '0 5px',
                            margin: 0,
                            background: _isLight ? '#fef3c7' : 'rgba(245,158,11,0.15)',
                            color: _isLight ? '#b45309' : '#fbbf24',
                            border: _isLight ? '1px solid #fde68a' : '1px solid rgba(245,158,11,0.3)'
                          }}
                        >
                          {pg2026T('admin_scheme_builtin', '内置')}
                        </Tag>
                      )}
                      <span
                        style={{
                          fontSize: 11,
                          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                          color: _isLight ? '#71717a' : '#a1a1aa',
                          background: _isLight ? '#f4f4f5' : '#27272a',
                          padding: '1px 6px',
                          borderRadius: 4,
                          border: _isLight ? '1px solid #e4e4e7' : '1px solid #3f3f46'
                        }}
                      >
                        ID: {scheme.id}
                      </span>
                    </div>
                    {isSelected ? (
                      <CheckCircleOutlined style={{ color: '#1677ff', fontSize: 16 }} />
                    ) : (
                      <div style={{ width: 16, height: 16, borderRadius: '50%', border: _isLight ? '1px solid #d4d4d8' : '1px solid #3f3f46' }} />
                    )}
                  </div>
                );
              })}
              {filteredSchemes.length === 0 && (
                <div style={{ textAlign: 'center', padding: '24px 0', color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 13 }}>{pg2026T('admin_no_matching_schemes', '暂无匹配类型的方案')}</div>
              )}
            </div>
          );
        })()}
      </Drawer>
    </div>
  );

  // ====== 体验方案配置 Tab ======
  const schemeListDense = name === 'playground_2026';
  const playgroundSchemeTab = (
    <div>
      <div style={{ background: _isLight ? '#fff' : '#141414', borderRadius: schemeListDense ? 6 : 8, padding: schemeListDense ? '8px 10px' : '16px 20px', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: schemeListDense ? 4 : 12, marginBottom: schemeListDense ? 6 : 16 }}>
          <div style={{ minWidth: schemeListDense ? 0 : 200, flex: '1 1 auto' }}>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: schemeListDense ? 13 : 14 }}>{pg2026T('admin_schemes_title', '创作方案列表')}</Text>
            {!schemeListDense && (
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, display: 'block', marginTop: 2 }}>
                {pg2026T('admin_schemes_desc', '管理内置和自定义的创作方案。每个方案定义了参数与 IO 管道，绑定到模型后生效。')}
              </Text>
            )}
          </div>
          <div style={{ display: 'flex', gap: schemeListDense ? 6 : 8, alignItems: 'center', flexWrap: 'wrap' }}>
            <Radio.Group value={pgSchemeTypeFilter} onChange={e => setPgSchemeTypeFilter(e.target.value)} size="small" style={{ flexShrink: 0 }}>
              <Radio.Button value="all">{pg2026T('admin_filter_all', '全部')}</Radio.Button>
              <Radio.Button value="chat">{pg2026T('admin_chat_title', '对话')}</Radio.Button>
              <Radio.Button value="image">{pg2026T('admin_io_asset_image', '图片')}</Radio.Button>
              <Radio.Button value="video">{pg2026T('admin_io_asset_video', '视频')}</Radio.Button>
              {name === 'playground_2026' && <Radio.Button value="audio">{pg2026T('admin_io_asset_audio', '音频')}</Radio.Button>}
            </Radio.Group>
            <Button type="primary" size="small" icon={<PlusOutlined />} onClick={handleAddScheme} style={{ flexShrink: 0 }}>{pg2026T('admin_scheme_new', '新增方案')}</Button>
          </div>
        </div>

        {(() => {
          const typeGroups: Record<string, { label: string; icon: React.ReactNode; color: string; schemes: { scheme: any; idx: number }[] }> = {
            video: { label: pg2026T('admin_scheme_group_video', '视频生成方案'), icon: <VideoCameraOutlined />, color: '#1677ff', schemes: [] },
            image: { label: pg2026T('admin_scheme_group_image', '图片生成方案'), icon: <PictureOutlined />, color: '#52c41a', schemes: [] },
            chat: { label: pg2026T('admin_scheme_group_chat', '聊天对话方案'), icon: <MessageOutlined />, color: '#722ed1', schemes: [] },
            audio: { label: pg2026T('admin_scheme_group_audio', '音频生成方案'), icon: <AudioOutlined />, color: '#13c2c2', schemes: [] },
            other: { label: pg2026T('admin_scheme_group_other', '其他方案'), icon: <AppstoreOutlined />, color: '#faad14', schemes: [] }
          };
          schemeList.forEach((scheme, idx) => {
            if (pgSchemeTypeFilter !== 'all') {
              if (pgSchemeTypeFilter === 'chat' && scheme.type !== 'chat') return;
              if (pgSchemeTypeFilter === 'video' && scheme.type !== 'video') return;
              if (pgSchemeTypeFilter === 'image' && scheme.type !== 'image') return;
              if (pgSchemeTypeFilter === 'audio' && scheme.type !== 'audio') return;
            }
            const key = scheme.type === 'video' ? 'video' : scheme.type === 'image' ? 'image' : scheme.type === 'chat' ? 'chat' : scheme.type === 'audio' ? 'audio' : 'other';
            typeGroups[key].schemes.push({ scheme, idx });
          });
          const activeGroups = Object.entries(typeGroups).filter(([, g]) => g.schemes.length > 0);
          if (activeGroups.length === 0) {
            return <div style={{ textAlign: 'center', padding: schemeListDense ? '16px 0' : '36px 0', color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 13 }}>{pg2026T('admin_scheme_empty', '暂无方案，点击「新增方案」创建')}</div>;
          }
          return activeGroups.map(([key, group]) => (
            <div key={key} style={{ marginBottom: schemeListDense ? 6 : 16 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginBottom: schemeListDense ? 3 : 8, paddingBottom: schemeListDense ? 2 : 6, borderBottom: `1px solid ${group.color}22` }}>
                <span style={{ color: group.color, fontSize: schemeListDense ? 12 : 14 }}>{group.icon}</span>
                <Text strong style={{ color: group.color, fontSize: 12 }}>{group.label}</Text>
                <Tag style={{ fontSize: 10, borderRadius: 10, lineHeight: '14px', padding: '0 5px', margin: 0, background: `${group.color}15`, border: `1px solid ${group.color}30`, color: group.color }}>{group.schemes.length}</Tag>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: schemeListDense ? 3 : 8 }}>
                {group.schemes.map(({ scheme, idx }) => {
                  const videoModes = (name === 'playground_2026' && scheme.type === 'video')
                    ? videoGenerationModesFromScheme(ensureSchemeIoDefaults(scheme))
                    : [];
                  const actionBtnH = schemeListDense ? 22 : 24;
                  return (
                    <div
                      key={scheme.id}
                      style={{
                        padding: schemeListDense ? '3px 8px' : '10px 14px',
                        borderRadius: schemeListDense ? 4 : 6,
                        border: _isLight ? '1px solid #e4e4e7' : '1px solid #27272a',
                        background: _isLight ? '#fafafa' : '#18181b',
                        display: 'flex',
                        flexDirection: 'column',
                        gap: schemeListDense ? 2 : 8,
                        transition: 'all 0.15s ease'
                      }}
                    >
                      {/* 首行：方案名称、内置标识、ID 以及 右侧操作按钮组 */}
                      <div
                        style={{
                          display: 'flex',
                          justifyContent: 'space-between',
                          alignItems: 'center',
                          flexWrap: 'wrap',
                          gap: schemeListDense ? 2 : 8,
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', gap: schemeListDense ? 4 : 6, flexWrap: 'wrap', minWidth: 0, flex: '1 1 auto' }}>
                          <Text strong style={{ color: _isLight ? '#09090b' : '#f4f4f5', fontSize: 13, lineHeight: schemeListDense ? '22px' : undefined, wordBreak: 'break-word' }}>
                            {scheme.name}
                          </Text>
                          {scheme.is_system && (
                            <Tag
                              style={{
                                fontSize: 10,
                                borderRadius: 4,
                                lineHeight: '14px',
                                padding: '0 4px',
                                margin: 0,
                                flexShrink: 0,
                                background: _isLight ? '#fef3c7' : 'rgba(245,158,11,0.15)',
                                color: _isLight ? '#b45309' : '#fbbf24',
                                border: _isLight ? '1px solid #fde68a' : '1px solid rgba(245,158,11,0.3)'
                              }}
                            >
                              {pg2026T('admin_scheme_builtin', '内置')}
                            </Tag>
                          )}
                          <span
                            style={{
                              fontSize: 11,
                              lineHeight: '14px',
                              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
                              color: _isLight ? '#71717a' : '#a1a1aa',
                              background: _isLight ? '#f4f4f5' : '#27272a',
                              padding: '0 5px',
                              borderRadius: 4,
                              border: _isLight ? '1px solid #e4e4e7' : '1px solid #3f3f46',
                              flexShrink: 0
                            }}
                          >
                            ID: {scheme.id}
                          </span>
                        </div>

                        {/* 右侧操作按钮 */}
                        <div style={{ display: 'flex', gap: 0, flexShrink: 0, alignItems: 'center', marginLeft: 'auto' }}>
                          <Button size="small" type="text" icon={<EditOutlined />} onClick={() => handleEditScheme(scheme, idx)} style={{ color: '#2563eb', padding: '0 4px', height: actionBtnH }}>{pg2026T('admin_scheme_edit', '编辑')}</Button>
                          <Tooltip title={scheme.type === 'image' || scheme.type === 'video' || (name === 'playground_2026' && (scheme.type === 'audio' || scheme.type === 'chat')) ? '' : pg2026T('admin_scheme_io_tip', (name === 'playground_2026' ? '仅图片、视频、音频、聊天方案支持 IO 配置' : '仅图片、视频方案支持 IO 配置'))}>
                            <span>
                              <Button
                                size="small"
                                type="text"
                                icon={<ApiOutlined />}
                                disabled={scheme.type !== 'image' && scheme.type !== 'video' && !(name === 'playground_2026' && (scheme.type === 'audio' || scheme.type === 'chat'))}
                                onClick={() => handleOpenSchemeIo(scheme, idx)}
                                style={{ color: scheme.type === 'image' || scheme.type === 'video' || (name === 'playground_2026' && (scheme.type === 'audio' || scheme.type === 'chat')) ? '#0d9488' : undefined, padding: '0 4px', height: actionBtnH }}
                              >
                                {pg2026T('admin_act_io', 'IO配置')}
                              </Button>
                            </span>
                          </Tooltip>
                          {scheme.is_system && (
                            <Button size="small" type="text" icon={<ReloadOutlined />} onClick={() => handleResetScheme(scheme.id, idx)} style={{ color: '#d97706', padding: '0 4px', height: actionBtnH }}>{pg2026T('admin_scheme_reset', '重置')}</Button>
                          )}
                          <Button size="small" type="text" icon={<DeleteOutlined />} onClick={() => handleDeleteScheme(idx)} danger disabled={!!scheme.is_system} style={{ padding: '0 4px', height: actionBtnH }}>{pg2026T('admin_scheme_delete', '删除')}</Button>
                        </div>
                      </div>

                      {/* 第二行：视频生成模式标签（文生 / 图生首帧 / 首尾帧 / 全能参考 等） */}
                      {videoModes.length > 0 && (
                        <div style={{ display: 'flex', flexWrap: 'wrap', gap: schemeListDense ? 3 : 5, alignItems: 'center' }}>
                          {videoModes.map((mode) => (
                            <Tag
                              key={mode.key}
                              style={{
                                fontSize: schemeListDense ? 10 : 11,
                                borderRadius: 4,
                                lineHeight: schemeListDense ? '16px' : '18px',
                                padding: schemeListDense ? '0 5px' : '0 6px',
                                margin: 0,
                                background: _isLight ? '#eff6ff' : 'rgba(37,99,235,0.18)',
                                color: _isLight ? '#1d4ed8' : '#93c5fd',
                                border: _isLight ? '1px solid #bfdbfe' : '1px solid rgba(59,130,246,0.35)',
                              }}
                            >
                              {mode.label}
                            </Tag>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          ));
        })()}

        {name !== 'playground_2026' && (
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 16 }}>
            <Button type="primary" loading={savingSchemes} onClick={handleSaveAllSchemes} icon={<SaveOutlined />}>
              保存全部方案
            </Button>
          </div>
        )}
      </div>

      {/* 方案编辑 Drawer */}
      <Drawer
        title={editingSchemeIndex >= 0 ? pg2026T('admin_scheme_drawer_edit', '编辑创作方案') : pg2026T('admin_scheme_drawer_new', '新建创作方案')}
        open={schemeEditVisible}
        onClose={() => {
          setSchemeEditVisible(false);
          setImageSpecialModalOpen(false);
        }}
        size="large"
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button onClick={() => setSchemeEditVisible(false)}>{pg2026T('admin_cancel', '取消')}</Button>
            <Button type="primary" loading={savingSchemes} onClick={handleSaveEditingScheme}>{pg2026T('admin_confirm', '确认')}</Button>
          </div>
        }
      >
        {editingScheme && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            {/* 基本信息 */}
            <div>
              <Text style={{ display: 'block', marginBottom: 6, fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{pg2026T('admin_scheme_name', '方案名称')}</Text>
              <Input value={editingScheme.name} onChange={e => setEditingScheme({ ...editingScheme, name: e.target.value })} />
            </div>
            <div style={{ display: 'flex', gap: 12 }}>
              <div style={{ flex: 1 }}>
                <Text style={{ display: 'block', marginBottom: 6, fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{pg2026T('admin_scheme_id', '方案 ID')}</Text>
                <Input value={editingScheme.id} onChange={e => setEditingScheme({ ...editingScheme, id: e.target.value })} disabled={!!editingScheme.is_system} />
              </div>
              <div style={{ flex: 1 }}>
                <Text style={{ display: 'block', marginBottom: 6, fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{pg2026T('admin_scheme_type', '类型')}</Text>
                <Select
                  value={editingScheme.type}
                  onChange={v => {
                    let next = { ...editingScheme, type: v };
                    if (name === 'playground_2026' && v === 'audio' && editingScheme.type !== 'audio') {
                      next = ensureSchemeIoDefaults({ ...next, inputs: [], outputs: [] });
                    } else if (v !== 'chat') {
                      const ioEmpty =
                        !Array.isArray(next.inputs) ||
                        next.inputs.length === 0 ||
                        !Array.isArray(next.outputs) ||
                        next.outputs.length === 0;
                      if (ioEmpty) next = ensureSchemeIoDefaults(next);
                    }
                    if (name === 'playground_2026' && (v === 'image' || v === 'video')) {
                      next.prompt_optimize = seedPromptOptimize(next.prompt_optimize, v);
                    }
                    if (name === 'playground_2026' && v === 'audio') {
                      next.voice_library = seedVoiceLibrary(next.voice_library, pgVoiceCatalogs);
                    }
                    setEditingScheme(next);
                  }}
                  style={{ width: '100%' }}
                  options={[
                    { label: pg2026T('admin_scheme_type_video', '视频 (video)'), value: 'video' },
                    { label: pg2026T('admin_scheme_type_image', '图片 (image)'), value: 'image' },
                    { label: pg2026T('admin_scheme_type_chat', '聊天 (chat)'), value: 'chat' },
                    ...(name === 'playground_2026' ? [{ label: pg2026T('admin_scheme_type_audio', '音频 (audio)'), value: 'audio' }] : []),
                  ]}
                />
              </div>
            </div>
            <div>
              <Text style={{ display: 'block', marginBottom: 6, fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{pg2026T('admin_scheme_desc', '描述')}</Text>
              <Input.TextArea value={editingScheme.description} onChange={e => setEditingScheme({ ...editingScheme, description: e.target.value })} autoSize={{ minRows: 2, maxRows: 4 }} />
            </div>

            <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />

            {name === 'playground_2026' && (editingScheme.type === 'image' || editingScheme.type === 'video') && (
              <>
                <SchemePromptOptimizeField
                  mode="scheme"
                  schemeType={editingScheme.type}
                  value={editingScheme.prompt_optimize}
                  isLight={_isLight}
                  onChange={(next) => setEditingScheme({ ...editingScheme, prompt_optimize: next || { enabled: false, body: '' } })}
                />
                <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
              </>
            )}

            {name === 'playground_2026' && editingScheme.type === 'audio' && (
              <>
                <SchemeVoiceLibraryField
                  mode="scheme"
                  schemeType={editingScheme.type}
                  value={editingScheme.voice_library}
                  catalogs={pgVoiceCatalogs}
                  isLight={_isLight}
                  onChange={(next) => setEditingScheme({ ...editingScheme, voice_library: next || seedVoiceLibrary({ enabled: false }, pgVoiceCatalogs) })}
                />
                <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
              </>
            )}

            {name === 'playground_2026' && editingScheme.type === 'image' && (
              <>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 }}>
                <div style={{ minWidth: 0 }}>
                  <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_isp_title', '图片专用参数配置')}</Text>
                  <Text style={{ display: 'block', fontSize: 12, marginTop: 2, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>
                    {pg2026T('admin_isp_desc_scheme', '开启后，图片生成页属性选择器展示比例、尺寸与分辨率专用控件')}
                  </Text>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                  {(isImageSpecialForcedOn(editingScheme.id) || !!editingScheme.image_special_params?.enabled) && (
                    <Button
                      size="small"
                      icon={<SettingOutlined />}
                      onClick={() => {
                        const next = seedImageSpecialParams(editingScheme.image_special_params);
                        setEditingScheme({
                          ...editingScheme,
                          image_special_params: next,
                        });
                        setImageSpecialBaseline(JSON.parse(JSON.stringify(next)));
                        setImageSpecialModalOpen(true);
                      }}
                    >
                      {pg2026T('admin_isp_btn_config', '配置')}
                    </Button>
                  )}
                  <Tooltip
                    title={isImageSpecialForcedOn(editingScheme.id) ? pg2026T('admin_isp_tip_forced', 'GPT-Image-2 按官方规则必须开启图片专用参数') : undefined}
                  >
                    <Switch
                      checked={isImageSpecialForcedOn(editingScheme.id) || !!editingScheme.image_special_params?.enabled}
                      disabled={isImageSpecialForcedOn(editingScheme.id)}
                      onChange={(v) => {
                        if (isImageSpecialForcedOn(editingScheme.id)) return;
                        if (!v) setImageSpecialModalOpen(false);
                        const prev = normalizeImageSpecialParams(editingScheme.image_special_params);
                        const next = v
                          ? seedImageSpecialParams(editingScheme.image_special_params)
                          : { ...prev, enabled: false };
                        setEditingScheme({ ...editingScheme, image_special_params: next });
                      }}
                    />
                  </Tooltip>
                </div>
              </div>
              <Divider style={{ margin: '8px 0', borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
              </>
            )}

            {/* 参数列表 */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_scheme_params_config', '参数配置')}</Text>
              <Button size="small" type="dashed" icon={<PlusOutlined />} onClick={handleAddParam}>{pg2026T('admin_scheme_add_param', '添加参数')}</Button>
            </div>

            {editingScheme.params?.map((param: any, pIdx: number) => {
              return (
              <div key={pIdx} style={{ background: _isLight ? '#fafafa' : '#1a1a1a', borderRadius: 8, padding: 14, border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)' }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
                  <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 12 }}>{pg2026T('admin_scheme_param_index', `参数 #${pIdx + 1}`, { index: pIdx + 1 })}</Text>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                    <Tooltip title={pg2026T('image_edit_layer_up', '上移')}>
                      <Button
                        type="text"
                        size="small"
                        icon={<ArrowUpOutlined />}
                        disabled={pIdx === 0}
                        onClick={() => handleMoveParam(pIdx, 'up')}
                      />
                    </Tooltip>
                    <Tooltip title={pg2026T('image_edit_layer_down', '下移')}>
                      <Button
                        type="text"
                        size="small"
                        icon={<ArrowDownOutlined />}
                        disabled={pIdx === (editingScheme.params?.length || 0) - 1}
                        onClick={() => handleMoveParam(pIdx, 'down')}
                      />
                    </Tooltip>
                    <Tooltip title={pg2026T('admin_scheme_delete', '删除')}>
                      <Button type="text" size="small" icon={<DeleteOutlined />} danger onClick={() => handleRemoveParam(pIdx)} />
                    </Tooltip>
                  </div>
                </div>
                <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                  <div style={{ flex: 1 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_key', 'Key')}</Text>
                    <Input size="small" value={param.key} onChange={e => handleEditingSchemeParamChange(pIdx, 'key', e.target.value)} />
                  </div>
                  <div style={{ flex: 1 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_label', '显示标签')}</Text>
                    <Input size="small" value={param.label} onChange={e => handleEditingSchemeParamChange(pIdx, 'label', e.target.value)} />
                    {isPg2026 && formatParamLabel(param) !== (param.label || '').trim() ? (
                      <Text style={{ fontSize: 11, display: 'block', marginTop: 4, color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)' }}>
                        {pg2026T('admin_option_preview', '用户端显示：{{labels}}', { labels: formatParamLabel(param) })}
                      </Text>
                    ) : null}
                  </div>
                  <div style={{ width: 140 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_control', '控件类型')}</Text>
                    <Select size="small" value={param.type} onChange={v => handleEditingSchemeParamChange(pIdx, 'type', v)} style={{ width: '100%' }}
                      options={name === 'playground_2026'
                        ? SCHEME_CONTROL_TYPE_OPTIONS.map(opt => ({ ...opt, label: pg2026T(`admin_ctrl_${opt.value}`, opt.label) }))
                        : [{ label: 'Input 文本', value: 'input' }, { label: 'Radio 单选', value: 'radio' }, { label: 'Select 下拉', value: 'select' }, { label: 'Switch 开关', value: 'switch' }, { label: 'Slider 滑块', value: 'slider' }]}
                    />
                  </div>
                  <div style={{ width: 120 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_datatype', '数据类型')}</Text>
                    <Select size="small" value={param.data_type || 'string'} onChange={v => handleEditingSchemeParamChange(pIdx, 'data_type', v)} style={{ width: '100%' }}
                      options={name === 'playground_2026'
                        ? [{ label: 'String', value: 'string' }, { label: 'Number', value: 'number' }, { label: 'Integer', value: 'integer' }, { label: 'Boolean', value: 'boolean' }]
                        : [{ label: 'String 字符串', value: 'string' }, { label: 'Number 数字', value: 'number' }, { label: 'Integer 整数', value: 'integer' }, { label: 'Boolean 布尔', value: 'boolean' }]}
                    />
                  </div>
                </div>
                {param.type !== 'switch' && param.type !== 'slider' && (
                  <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                    <div style={{ flex: 1 }}>
                      <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>
                        {pg2026T('admin_override_field_options_comma', '选项列表（用英文逗号分隔）')}
                      </Text>
                      <Input
                        size="small"
                        value={Array.isArray(param.options) ? param.options.join(',') : ''}
                        onChange={(e) => {
                          const { options, option_labels: parsedLabels } = parseOptionsAndLabels(e.target.value);
                          const mergedLabels = { ...(param.option_labels || {}), ...parsedLabels };
                          handleEditingSchemeParamBatchChange(pIdx, {
                            options,
                            option_labels: Object.keys(mergedLabels).length > 0 ? mergedLabels : undefined,
                          });
                        }}
                        placeholder={pg2026T('admin_override_field_options_ph', '例如: opaque,transparent 或 16:9,9:16')}
                      />
                    </div>
                    <div style={{ flex: 1 }}>
                      <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>
                        {pg2026T('admin_override_field_mapping', '选项中文映射（可选，例如 opaque:不透明,transparent:透明 或 不透明,透明）')}
                      </Text>
                      <Input
                        size="small"
                        value={formatOptionLabelsDisplay(param.option_labels, param.options)}
                        onChange={(e) => {
                          const newLabels = parseOptionLabelsMapping(e.target.value, param.options || []);
                          handleEditingSchemeParamChange(
                            pIdx,
                            'option_labels',
                            Object.keys(newLabels).length > 0 ? newLabels : undefined,
                          );
                        }}
                        placeholder={pg2026T('admin_override_field_mapping_ph', '例如: opaque:不透明,transparent:透明 或 不透明,透明')}
                      />
                      {renderPgOptionPreview(param)}
                    </div>
                  </div>
                )}
                {param.type === 'slider' && (
                  <div style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                    <div style={{ flex: 1 }}>
                      <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_min', '最小值')}</Text>
                      <InputNumber size="small" style={{ width: '100%' }} value={param.min ?? 0}
                        onChange={v => handleEditingSchemeParamChange(pIdx, 'min', v ?? 0)}
                      />
                    </div>
                    <div style={{ flex: 1 }}>
                      <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_max', '最大值')}</Text>
                      <InputNumber size="small" style={{ width: '100%' }} value={param.max ?? 100}
                        onChange={v => handleEditingSchemeParamChange(pIdx, 'max', v ?? 100)}
                      />
                    </div>
                    <div style={{ flex: 1 }}>
                      <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_step', '步长')}</Text>
                      <InputNumber size="small" style={{ width: '100%' }} value={param.step ?? 1} min={0.001}
                        onChange={v => handleEditingSchemeParamChange(pIdx, 'step', v ?? 1)}
                      />
                    </div>
                  </div>
                )}
                <div style={{ display: 'flex', gap: 8 }}>
                  <div style={{ flex: 1 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_default', '默认值')}</Text>
                    {param.type === 'switch' ? (
                      <Switch checked={!!param.default} onChange={v => handleEditingSchemeParamChange(pIdx, 'default', v)} />
                    ) : param.type === 'slider' ? (
                      <InputNumber size="small" style={{ width: '100%' }}
                        value={typeof param.default === 'number' ? param.default : Number(param.default) || 0}
                        min={param.min ?? 0} max={param.max ?? 100} step={param.step ?? 1}
                        onChange={v => handleEditingSchemeParamChange(pIdx, 'default', v ?? 0)}
                      />
                    ) : (
                      <Input size="small" value={String(param.default ?? '')} onChange={e => handleEditingSchemeParamChange(pIdx, 'default', e.target.value)} />
                    )}
                  </div>
                  <div style={{ width: 100 }}>
                    <Text style={{ display: 'block', marginBottom: 4, fontSize: 11, color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)' }}>{pg2026T('admin_override_field_unit', '单位')}</Text>
                    <Input size="small" value={param.unit || ''} onChange={e => handleEditingSchemeParamChange(pIdx, 'unit', e.target.value)} placeholder={pg2026T('admin_override_field_unit_ph', '可选')} />
                  </div>
                </div>
                {name === 'playground_2026' && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 10 }}>
                    <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>{pg2026T('admin_override_param_on', '模型默认启用')}</Text>
                    <Switch
                      size="small"
                      checked={param.enabled !== false}
                      onChange={(v) => handleEditingSchemeParamChange(pIdx, 'enabled', v)}
                    />
                  </div>
                )}
                {name === 'playground_2026' && (editingScheme.type === 'image' || editingScheme.type === 'video' || editingScheme.type === 'audio') && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 10 }}>
                    <Text style={{ fontSize: 12, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>{pg2026T('admin_override_quick_bar', '快捷栏')}</Text>
                    <Tooltip
                      title={pg2026T('admin_quick_bar_help', SCHEME_QUICK_BAR_HELP)}
                    >
                      <QuestionCircleOutlined style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', cursor: 'help' }} />
                    </Tooltip>
                    <Switch
                      size="small"
                      checked={!!param.quick}
                      disabled={!isQuickBarEligible(param) && !param.quick}
                      onChange={(v) => handleEditingSchemeParamChange(pIdx, 'quick', v)}
                    />
                  </div>
                )}
              </div>
              );
            })}
          </div>
        )}
      </Drawer>

      {/* 方案 IO 配置 Modal */}
      <Modal
        title={pg2026T('admin_scheme_io_title', `IO配置 — ${editingIoScheme?.name || ''}`, { name: editingIoScheme?.name || '' })}
        open={schemeIoEditVisible}
        onCancel={() => setSchemeIoEditVisible(false)}
        width={780}
        style={{ top: 40 }}
        bodyStyle={{ maxHeight: 'calc(80vh - 120px)', overflowY: 'auto' }}
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
            <Button onClick={() => setSchemeIoEditVisible(false)}>{pg2026T('admin_cancel', '取消')}</Button>
            <Button type="primary" loading={savingSchemes} onClick={handleSaveSchemeIo}>{pg2026T('admin_confirm', '确认')}</Button>
          </div>
        }
      >
        {editingIoScheme && (
          <SchemeIoEditor
            standalone
            localizeLabels={isPg2026}
            schemeId={editingIoScheme.id}
            schemeType={editingIoScheme.type}
            params={editingIoScheme.params || []}
            inputs={editingIoScheme.inputs || []}
            outputs={editingIoScheme.outputs || []}
            onChange={(next) => setEditingIoScheme({ ...editingIoScheme, ...next })}
            isLight={_isLight}
          />
        )}
      </Modal>

      {name === 'playground_2026' && (
        <Modal
          title={pg2026T('admin_isp_title', '图片专用参数配置')}
          open={imageSpecialModalOpen}
          onCancel={() => {
            setEditingScheme((prev: any) => (prev ? { ...prev, image_special_params: imageSpecialBaseline } : prev));
            setImageSpecialModalOpen(false);
          }}
          width={640}
          footer={
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
              <Button
                onClick={() => {
                  setEditingScheme((prev: any) => (prev ? { ...prev, image_special_params: officialImageSpecialParams(prev.id) } : prev));
                }}
              >
                {pg2026T('admin_reset', '重置')}
              </Button>
              <Button
                onClick={() => {
                  setEditingScheme((prev: any) => (prev ? { ...prev, image_special_params: imageSpecialBaseline } : prev));
                  setImageSpecialModalOpen(false);
                }}
              >
                {pg2026T('admin_cancel', '取消')}
              </Button>
              <Button type="primary" onClick={() => setImageSpecialModalOpen(false)}>{pg2026T('admin_done', '完成')}</Button>
            </div>
          }
        >
          <ImageSpecialParamsEditor
            value={editingScheme?.image_special_params}
            params={editingScheme?.params || []}
            onChange={(next) => setEditingScheme((prev: any) => (prev ? { ...prev, image_special_params: next } : prev))}
            isLight={_isLight}
          />
        </Modal>
      )}
    </div>
  );

  const playgroundAdvancedNodesTab = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* 功能总控 Card */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
          <div>
            <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 16, fontWeight: 600 }}>
              开启高级节点功能
            </div>
            <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 13, marginTop: 4 }}>
              开启后，用户在创作中心画布上点击选中的图片或视频素材时，可以追加高级操作节点进行处理。
            </div>
          </div>
          <Switch
            checked={pgAdvancedNodesEnabled}
            onChange={(checked) => setPgAdvancedNodesEnabled(checked)}
            style={{ transform: 'scale(1.1)' }}
          />
        </div>
      </div>

      {/* 基础实例设置 */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 16, fontWeight: 600 }}>
              基础素材实例上限
            </div>
            <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 13, marginTop: 4 }}>
              设置在创作中心画布上最多可以存在的图片或视频基础素材节点的数量。
            </div>
          </div>
          <Space>
            <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
            <InputNumber 
              size="middle" 
              min={1} 
              max={1000} 
              value={pgAdvancedNodeInstanceLimit} 
              onChange={(val) => setPgAdvancedNodeInstanceLimit(val || 50)} 
            />
          </Space>
        </div>
      </div>

      {/* 节点列表 Card (总开关开启时可用) */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        opacity: pgAdvancedNodesEnabled ? 1 : 0.6,
        pointerEvents: pgAdvancedNodesEnabled ? 'auto' : 'none',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 15, fontWeight: 600 }}>
            可用的节点列表
          </div>
          <Space>
            <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>一键统一配置上限</span>
            <Switch
              size="small"
              checked={pgAdvancedNodesUnifiedLimitEnabled}
              onChange={setPgAdvancedNodesUnifiedLimitEnabled}
            />
            {pgAdvancedNodesUnifiedLimitEnabled && (
              <InputNumber
                size="small"
                min={1}
                max={100}
                value={pgAdvancedNodesUnifiedLimitValue}
                onChange={(val) => setPgAdvancedNodesUnifiedLimitValue(val || 10)}
              />
            )}
          </Space>
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          
          {/* 系统节点：预览节点 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            transition: 'border-color 0.2s',
          }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>预览节点</span>
                <span style={{
                  fontSize: 11,
                  background: 'rgba(82,196,26,0.1)',
                  color: '#52c41a',
                  border: '1px solid rgba(82,196,26,0.2)',
                  borderRadius: 4,
                  padding: '1px 6px',
                  fontWeight: 500,
                }}>
                  系统节点
                </span>
              </div>
              <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                生成当前素材的快速缩略图、比例预览和预览文件。
              </div>
            </div>
            <Space size="large">
              <Space>
                <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                <InputNumber 
                  size="small" 
                  min={1} 
                  max={100} 
                  value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodePreviewLimit} 
                  onChange={(val) => setPgAdvancedNodePreviewLimit(val || 10)} 
                  disabled={pgAdvancedNodesUnifiedLimitEnabled}
                />
              </Space>
              <Switch
                checked={pgAdvancedNodePreviewEnabled}
                onChange={(checked) => setPgAdvancedNodePreviewEnabled(checked)}
                size="small"
              />
            </Space>
          </div>

          {/* 系统节点：提示词节点 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            transition: 'border-color 0.2s',
          }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>提示词节点</span>
                <span style={{
                  fontSize: 11,
                  background: 'rgba(82,196,26,0.1)',
                  color: '#52c41a',
                  border: '1px solid rgba(82,196,26,0.2)',
                  borderRadius: 4,
                  padding: '1px 6px',
                  fontWeight: 500,
                }}>
                  系统节点
                </span>
              </div>
              <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                在画布上建立并管理专门的文本提示词编辑区域。
              </div>
            </div>
            <Space size="large">
              <Space>
                <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                <InputNumber 
                  size="small" 
                  min={1} 
                  max={100} 
                  value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodePromptLimit} 
                  onChange={(val) => setPgAdvancedNodePromptLimit(val || 10)} 
                  disabled={pgAdvancedNodesUnifiedLimitEnabled}
                />
              </Space>
              <Switch
                checked={pgAdvancedNodePromptEnabled}
                onChange={(checked) => setPgAdvancedNodePromptEnabled(checked)}
                size="small"
              />
            </Space>
          </div>

          {/* 系统节点：AI视频节点 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            transition: 'border-color 0.2s',
          }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>AI视频节点</span>
                <span style={{
                  fontSize: 11,
                  background: 'rgba(82,196,26,0.1)',
                  color: '#52c41a',
                  border: '1px solid rgba(82,196,26,0.2)',
                  borderRadius: 4,
                  padding: '1px 6px',
                  fontWeight: 500,
                }}>
                  系统节点
                </span>
              </div>
              <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                运行AI生成算法以生成或编辑视频。
              </div>
            </div>
            <Space size="large">
              <Space>
                <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                <InputNumber 
                  size="small" 
                  min={1} 
                  max={100} 
                  value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAiVideoLimit} 
                  onChange={(val) => setPgAdvancedNodeAiVideoLimit(val || 10)} 
                  disabled={pgAdvancedNodesUnifiedLimitEnabled}
                />
              </Space>
              <Switch
                checked={pgAdvancedNodeAiVideoEnabled}
                onChange={(checked) => setPgAdvancedNodeAiVideoEnabled(checked)}
                size="small"
              />
            </Space>
          </div>

          {/* 系统节点：AI图片节点 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            transition: 'border-color 0.2s',
          }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>AI图片节点</span>
                <span style={{
                  fontSize: 11,
                  background: 'rgba(82,196,26,0.1)',
                  color: '#52c41a',
                  border: '1px solid rgba(82,196,26,0.2)',
                  borderRadius: 4,
                  padding: '1px 6px',
                  fontWeight: 500,
                }}>
                  系统节点
                </span>
              </div>
              <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                运行AI生成算法以生成或编辑图像。
              </div>
            </div>
            <Space size="large">
              <Space>
                <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                <InputNumber 
                  size="small" 
                  min={1} 
                  max={100} 
                  value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAiImageLimit} 
                  onChange={(val) => setPgAdvancedNodeAiImageLimit(val || 10)} 
                  disabled={pgAdvancedNodesUnifiedLimitEnabled}
                />
              </Space>
              <Switch
                checked={pgAdvancedNodeAiImageEnabled}
                onChange={(checked) => setPgAdvancedNodeAiImageEnabled(checked)}
                size="small"
              />
            </Space>
          </div>

          {/* 增强节点：Agent 智能体 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            transition: 'border-color 0.2s',
          }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>Agent智能体</span>
                <span style={{
                  fontSize: 11,
                  background: 'rgba(24,144,255,0.1)',
                  color: '#1890ff',
                  border: '1px solid rgba(24,144,255,0.2)',
                  borderRadius: 4,
                  padding: '1px 6px',
                  fontWeight: 500,
                }}>
                  增强节点
                </span>
              </div>
              <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                在画布上建立独立智能体对话，支持设定系统提示词并执行特定角色流式响应。
              </div>
            </div>
            <Space size="large">
              <Space>
                <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                <InputNumber 
                  size="small" 
                  min={1} 
                  max={100} 
                  value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeAgentLimit} 
                  onChange={(val) => setPgAdvancedNodeAgentLimit(val || 10)} 
                  disabled={pgAdvancedNodesUnifiedLimitEnabled}
                />
              </Space>
              <Switch
                checked={pgAdvancedNodeAgentEnabled}
                onChange={(checked) => setPgAdvancedNodeAgentEnabled(checked)}
                size="small"
              />
            </Space>
          </div>

          {/* 增强节点：火山画质增强 */}
          <div style={{
            background: _isLight ? 'rgba(0,0,0,0.01)' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 8,
            padding: '16px',
            transition: 'all 0.2s',
            opacity: volcEnhancePluginActive ? 1 : 0.85,
          }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14, fontWeight: 500 }}>火山画质增强</span>
                  <span style={{
                    fontSize: 11,
                    background: 'rgba(24,144,255,0.1)',
                    color: '#1890ff',
                    border: '1px solid rgba(24,144,255,0.2)',
                    borderRadius: 4,
                    padding: '1px 6px',
                    fontWeight: 500,
                  }}>
                    增强节点
                  </span>
                </div>
                <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginTop: 4 }}>
                  通过接入火山引擎 MediaKit 进行视频或图片的超分辨率、插帧及智能修复。
                </div>
              </div>
              <Space size="large">
                <Space>
                  <span style={{ fontSize: 13, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)' }}>数量上限</span>
                  <InputNumber 
                    size="small" 
                    min={1} 
                    max={100} 
                    value={pgAdvancedNodesUnifiedLimitEnabled ? pgAdvancedNodesUnifiedLimitValue : pgAdvancedNodeVolcEnhanceLimit} 
                    onChange={(val) => setPgAdvancedNodeVolcEnhanceLimit(val || 10)} 
                    disabled={pgAdvancedNodesUnifiedLimitEnabled || !volcEnhancePluginActive}
                  />
                </Space>
                <Tooltip title={!volcEnhancePluginActive ? "依赖的前置“AI MediaKit火山引擎画质增加插件”未开启" : ""}>
                  <Switch
                    checked={pgAdvancedNodeVolcEnhanceEnabled}
                    onChange={(checked) => setPgAdvancedNodeVolcEnhanceEnabled(checked)}
                    disabled={!volcEnhancePluginActive}
                    size="small"
                  />
                </Tooltip>
              </Space>
            </div>

            {/* 如果依赖插件未激活，显示警告及指引链接 */}
            {!volcEnhancePluginActive && (
              <div style={{
                marginTop: 12,
                background: _isLight ? 'rgba(250,173,20,0.06)' : 'rgba(250,173,20,0.1)',
                border: '1px solid rgba(250,173,20,0.2)',
                borderRadius: 6,
                padding: '8px 12px',
                fontSize: 12,
                color: _isLight ? '#d46b08' : '#e08f23',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}>
                <span>
                  ⚠️ 该功能依赖 <strong>AI MediaKit火山引擎画质增加插件</strong>，检测到当前未启用。
                </span>
                <a
                  href={`/${adminPath}/plugins`}
                  onClick={(e) => {
                    e.preventDefault();
                    navigate(`/${adminPath}/plugins`);
                  }}
                  style={{
                    color: '#1890ff',
                    fontWeight: 500,
                    cursor: 'pointer',
                    textDecoration: 'underline',
                  }}
                >
                  去开启插件
                </a>
              </div>
            )}
          </div>

        </div>
      </div>

      {/* 保存按钮卡片 */}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <Button
          type="primary"
          loading={savingPlayground}
          onClick={handleSavePlaygroundConfig}
          icon={<SaveOutlined />}
        >
          保存高级节点配置
        </Button>
      </div>
    </div>
  );

  const playgroundAgentConfigTab = (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* 功能总控 Card */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
          <div>
            <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 16, fontWeight: 600 }}>
              开启智能体模式高级功能
            </div>
            <div style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 13, marginTop: 4 }}>
              开启后，用户在创作中心左上角和右下角的菜单中可以切换到「AI智能体模式」，并支持高级智能体操作。
            </div>
          </div>
          <Switch
            checked={pgAgentModeEnabled}
            onChange={(checked) => setPgAgentModeEnabled(checked)}
            style={{ transform: 'scale(1.1)' }}
          />
        </div>
      </div>

      {/* 视频生成模式选择 Card (总开关开启时可用) */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        opacity: pgAgentModeEnabled ? 1 : 0.6,
        pointerEvents: pgAgentModeEnabled ? 'auto' : 'none',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 15, fontWeight: 600, marginBottom: 16 }}>
          智能体操作视频生成模式
        </div>
        <Radio.Group 
          value={pgAgentVideoMode} 
          onChange={(e) => setPgAgentVideoMode(e.target.value)}
          style={{ width: '100%', display: 'flex', flexDirection: 'column', gap: 16 }}
        >
          <Radio value="track" style={{ display: 'flex', alignItems: 'flex-start', margin: 0 }}>
            <div style={{ marginLeft: 8 }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', display: 'block', fontSize: 14 }}>操作轨迹视频生成</Text>
              <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 4 }}>智能体根据用户在创作中心画布上的一系列节点操作轨迹，自动渲染生成连贯性操作视频</Text>
            </div>
          </Radio>
          
          <Radio value="autonomous" style={{ display: 'flex', alignItems: 'flex-start', margin: 0 }}>
            <div style={{ marginLeft: 8 }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', display: 'block', fontSize: 14 }}>自主决策视频生成</Text>
              <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 4 }}>智能体根据用户的输入指令，自主进行规划、点击操作节点并合成最终的演示生成视频</Text>
            </div>
          </Radio>

          <Radio value="interactive" style={{ display: 'flex', alignItems: 'flex-start', margin: 0 }}>
            <div style={{ marginLeft: 8 }}>
              <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', display: 'block', fontSize: 14 }}>交互录制视频生成</Text>
              <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 4 }}>智能体实时响应交互，捕获当前画布视口的内容并以录屏形式导出高质量操作演示视频</Text>
            </div>
          </Radio>
        </Radio.Group>
      </div>

      {/* 对话界面高度自定义配置 Card */}
      <div style={{
        background: _isLight ? '#fff' : '#141414',
        borderRadius: 12,
        padding: '24px',
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)',
        boxShadow: _isLight ? '0 2px 8px rgba(0,0,0,0.02)' : '0 2px 8px rgba(0,0,0,0.2)',
        opacity: pgAgentModeEnabled ? 1 : 0.6,
        pointerEvents: pgAgentModeEnabled ? 'auto' : 'none',
        transition: 'all 0.3s ease',
      }}>
        <div style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 15, fontWeight: 600, marginBottom: 16 }}>
          对话沉浸式界面配置
        </div>
        
        <div style={{ marginBottom: 16 }}>
          <div style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, marginBottom: 8 }}>
            欢迎语大标题 (默认: Start a conversation)
          </div>
          <Input 
            placeholder="Start a conversation" 
            value={pgAgentWelcomeTitle} 
            onChange={e => setPgAgentWelcomeTitle(e.target.value)} 
          />
        </div>

        <div style={{ marginBottom: 16 }}>
          <div style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, marginBottom: 8 }}>
            欢迎语副标题 (默认: I can help you design, and optimise your creative workflow.)
          </div>
          <Input 
            placeholder="I can help you design, and optimise your creative workflow." 
            value={pgAgentWelcomeDesc} 
            onChange={e => setPgAgentWelcomeDesc(e.target.value)} 
          />
        </div>

        <div style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <div style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13 }}>
              智能体系统提示词 (限定使用范围)
            </div>
            <Button size="small" onClick={() => setPgAgentSystemPrompt("你是一个专业的创作中心 AI 智能体，专为辅助用户制作图片和视频而设计。你的主要任务是根据用户的需求，通过对话理解他们的意图，并利用站点提供的内置节点帮助用户自动搭建工作流，最终用户只需点击生成即可完成创作。\n\n【限定使用范围】\n1. 只能回答与图片制作、视频生成、创意设计相关的创作类问题。\n2. 对于任何超出创作中心和音视频/图片生成范畴的话题（如政治、编程、代码、常识问答等），请礼貌地拒绝，并引导用户回到图像/视频创作上。\n3. 保持热情、专业，并引导用户提供更具体的画面描述以优化生成效果。")}>
              填入 Hermes 风格默认提示词
            </Button>
          </div>
          <Input.TextArea
            autoSize={{ minRows: 4, maxRows: 8 }}
            placeholder="请在此输入限定智能体行为和范围的 System Prompt。如果不填则使用系统全局默认配置。"
            value={pgAgentSystemPrompt}
            onChange={e => setPgAgentSystemPrompt(e.target.value)}
          />
        </div>

        <div style={{ marginBottom: 16 }}>
          <div style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, marginBottom: 8 }}>
            智能体对话大模型 (多选)
          </div>
          <Select
            mode="multiple"
            style={{ width: '100%' }}
            placeholder="请选择智能体聊天可选的大模型"
            value={pgAgentChatModels}
            onChange={setPgAgentChatModels}
            options={pgModels.filter(m => m.scheme_type === 'chat' || m.type_name?.includes('聊天') || m.type_name?.includes('对话')).map(m => ({ label: m.name, value: m.mid }))}
          />
        </div>

        <div>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
            <div style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13 }}>
              预设引导提示词 (将展示为对话框上方的点击卡片)
            </div>
            <Button size="small" onClick={() => setPgAgentPresetPrompts([
              { icon: '🎬', text: '我想制作一段赛博朋克风格的科幻视频' },
              { icon: '👩', text: '帮我生成一张极具质感的人像照片' },
              { icon: '🎨', text: '根据我的描述设计一个 3D IP 盲盒角色' }
            ])}>
              填入 Hermes 风格预设卡片
            </Button>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {pgAgentPresetPrompts.map((prompt, index) => (
              <div key={index} style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <Input 
                  placeholder="图标 (如 🖼️)" 
                  style={{ width: 80 }} 
                  value={prompt.icon} 
                  onChange={e => {
                    const newPrompts = [...pgAgentPresetPrompts];
                    newPrompts[index] = { ...newPrompts[index], icon: e.target.value };
                    setPgAgentPresetPrompts(newPrompts);
                  }} 
                />
                <Input 
                  placeholder="提示词内容 (如 Create a product shot...)" 
                  value={prompt.text} 
                  onChange={e => {
                    const newPrompts = [...pgAgentPresetPrompts];
                    newPrompts[index] = { ...newPrompts[index], text: e.target.value };
                    setPgAgentPresetPrompts(newPrompts);
                  }} 
                />
                <Button 
                  danger 
                  type="text" 
                  icon={<DeleteOutlined />} 
                  onClick={() => {
                    const newPrompts = [...pgAgentPresetPrompts];
                    newPrompts.splice(index, 1);
                    setPgAgentPresetPrompts(newPrompts);
                  }} 
                />
              </div>
            ))}
            <Button 
              type="dashed" 
              icon={<PlusOutlined />} 
              onClick={() => {
                setPgAgentPresetPrompts([...pgAgentPresetPrompts, { icon: '', text: '' }]);
              }}
              style={{ width: '100%', marginTop: 8 }}
            >
              添加预设提示词
            </Button>
          </div>
        </div>
      </div>

      {/* 保存按钮卡片 */}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <Button
          type="primary"
          loading={savingPlayground}
          onClick={handleSavePlaygroundConfig}
          icon={<SaveOutlined />}
        >
          保存智能体配置
        </Button>
      </div>
    </div>
  );

  const handleSaveWorkflowConfig = async () => {
    try {
      setSavingWorkflowConfig(true);
      await request.post(`/plugins/${name}/config`, {
        allowed_levels: isAllLevels ? 'all' : selectedLevels.join(','),
        level_quotas: levelQuotas,
        default_quota: defaultQuota,
        level_max_folders: levelMaxFolders,
        default_max_folders: defaultMaxFolders,
        level_max_files_per_folder: levelMaxFilesPerFolder,
        default_max_files_per_folder: defaultMaxFilesPerFolder,
        level_max_projects: levelMaxProjects,
        default_max_projects: defaultMaxProjects,
        level_max_assets: levelMaxAssets,
        default_max_assets: defaultMaxAssets,
        workflow_node_limit: workflowNodeLimit,
        workflow_enabled: workflowEnabled,
        workflow_menu_title: workflowMenuTitle.trim(),
        user_nav_modules: userNavModules,
        volc_enhance_enabled: pgAdvancedNodeVolcEnhanceEnabled,
        director_enabled: pgAdvancedNodeDirectorEnabled,
        show_in_playground_prompt: showInPlaygroundPrompt,
        docs_api_allow_guest: docsApiAllowGuest,
        show_in_admin_menu: showInAdminMenu ? 1 : 0,
        admin_menu_sort: adminMenuSort,
        admin_menu_title: adminMenuTitle.trim(),
        admin_menu_default_tab: resolvePluginAdminDefaultTab(
          plugin?.name || name || '',
          adminMenuDefaultTab,
          name ? dynamicPlugins[name] : undefined,
        ),
      });
      // 同步保存到 playground-config
      try {
        await request.post(`/plugins/${name}/playground-config`, {
          default_model_mids: pgDefaultModelMids,
          models: pgModels.map(m => ({
            id: m.id,
            mid: m.mid || '',
            enabled: m.pg_enabled,
            scheme_id: m.pg_scheme_id || null,
            param_overrides: m.pg_param_overrides || null,
            io_overrides: m.pg_io_overrides || null,
            sort_order: m.pg_sort_order || 0,
            ...(featureKindFromTypeName(m.type_name) ? {
              feature_attributes: parseFeatureAttrList(m.feature_attributes),
              feature_keys: Array.isArray(m.pg_feature_keys) ? m.pg_feature_keys : [],
            } : {}),
            ...(name === 'playground_2026' && featureKindFromTypeName(m.type_name) === 'chat' ? {
              ...(typeof m.pg_input_protocol === 'string' && m.pg_input_protocol
                ? { input_protocol: m.pg_input_protocol }
                : {}),
              ...(typeof m.pg_thinking_profile === 'string' && m.pg_thinking_profile
                ? { thinking_profile: m.pg_thinking_profile }
                : {}),
            } : {}),
          })),
          advanced_nodes: {
            enabled: true,
            preview_enabled: true,
            volc_enhance_enabled: pgAdvancedNodeVolcEnhanceEnabled,
            director_enabled: pgAdvancedNodeDirectorEnabled,
            prompt_enabled: true,
            ai_video_enabled: true,
            ai_image_enabled: true,
            agent_enabled: false,
          },
        });
      } catch (e) {
        console.error('sync playground-config failed', e);
      }
      message.success(pg2026T('admin_wf_save_ok', '工作流配置已保存'));
    } catch (e: any) {
      message.error(name === 'playground_2026' ? `${pg2026T('admin_sort_save_failed', '保存失败')}: ${e?.message || ''}` : ('保存失败: ' + (e?.message || '未知错误')));
    } finally {
      setSavingWorkflowConfig(false);
    }
  };

  const playgroundWorkflowConfigTab = (
    <div>
      <div style={{
        background: _isLight ? '#fff' : '#141414', borderRadius: 8,
        border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', padding: '20px', marginBottom: 16
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 8, flexWrap: 'wrap', gap: 12 }}>
          <div>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>{pg2026T('admin_wf_title', '工作流管理')}</Text><br />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              {pg2026T('admin_wf_desc', '控制是否开放工作流功能、单个工作流画布上的节点数量上限，以及是否开放「火山增强」「导演台」节点')}
            </Text>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 13, color: workflowEnabled ? '#1677ff' : (_isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)') }}>
              {workflowEnabled ? pg2026T('admin_wf_status_on', '已开启工作流') : pg2026T('admin_wf_status_off', '未开启工作流')}
            </span>
            <Switch
              checked={workflowEnabled}
              onChange={(checked) => setUserNavModules((prev) => patchUserNavModule(prev, 'workflow', { enabled: checked }))}
            />
          </div>
        </div>

        {!workflowEnabled && (
          <div style={{
            margin: '12px 0',
            background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.03)',
            border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
            borderRadius: 6,
            padding: '8px 12px',
            fontSize: 12,
            color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)',
          }}>
            {pg2026T('admin_wf_disabled_tip', '💡 当前工作流总开关处于关闭状态，前台用户端侧栏将隐藏「工作流」菜单与额度入口。')}
          </div>
        )}

        <Divider style={{ borderColor: _isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)', margin: '14px 0' }} />
        <div style={{ display: 'flex', alignItems: 'center', gap: 16, flexWrap: 'wrap' }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, minWidth: 140 }}>
            {pg2026T('admin_wf_menu_name', '用户端菜单名称')}
          </Text>
          <Input
            size="small"
            placeholder={pg2026T('admin_wf_menu_ph', '工作流')}
            value={workflowMenuTitle}
            onChange={(e) => setUserNavModules((prev) => patchUserNavModule(prev, 'workflow', { title: e.target.value }))}
            style={{ width: 160 }}
            allowClear
          />
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
            {pg2026T('admin_wf_menu_hint', '自定义用户端创作中心 2026 侧边栏及页面展示的菜单名称，留空默认为「工作流」')}
          </Text>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16, flexWrap: 'wrap', marginTop: 14 }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, minWidth: 140 }}>
            {pg2026T('admin_wf_node_limit', '单个工作流节点上限')}
          </Text>
          <InputNumber
            size="small"
            min={1}
            max={5000}
            value={workflowNodeLimit}
            onChange={(val) => setWorkflowNodeLimit(val ?? 200)}
            style={{ width: 160 }}
            addonAfter={pg2026T('admin_wf_node_unit', '个')}
          />
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16, flexWrap: 'wrap', marginTop: 14 }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, minWidth: 140 }}>
            {pg2026T('admin_wf_volc', '火山画质增强')}
          </Text>
          <Tooltip title={!volcEnhancePluginActive ? pg2026T('admin_wf_volc_tip_disabled', '依赖的「AI MediaKit 火山引擎画质增强」插件未开启') : pg2026T('admin_wf_volc_tip_enabled', '关闭后工作流添加列表不显示该节点，画布上已有节点也不可提交')}>
            <Switch
              checked={pgAdvancedNodeVolcEnhanceEnabled}
              onChange={(checked) => setPgAdvancedNodeVolcEnhanceEnabled(checked)}
              disabled={!volcEnhancePluginActive}
              size="small"
            />
          </Tooltip>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
            {volcEnhancePluginActive
              ? pg2026T('admin_wf_volc_hint_on', '关闭后不显示、不可用火山画质增强节点')
              : pg2026T('admin_wf_volc_hint_off', '请先启用火山引擎画质增强插件')}
          </Text>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 16, flexWrap: 'wrap', marginTop: 14 }}>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, minWidth: 140 }}>
            {pg2026T('admin_wf_director', '导演台')}
          </Text>
          <Tooltip title={pg2026T('admin_wf_director_tip', '关闭后工作流添加列表不显示导演台节点')}>
            <Switch
              checked={pgAdvancedNodeDirectorEnabled}
              onChange={(checked) => setPgAdvancedNodeDirectorEnabled(checked)}
              size="small"
            />
          </Tooltip>
          <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
            {pg2026T('admin_wf_director_hint', '默认关闭；开启后才可在工作流中添加导演台节点')}
          </Text>
        </div>
      </div>

      <PluginModule>
        <PlaygroundDemoImageConfig />
      </PluginModule>

      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <Button
          type="primary"
          loading={savingWorkflowConfig}
          onClick={handleSaveWorkflowConfig}
          icon={<SaveOutlined />}
        >
          {pg2026T('admin_wf_save_btn', '保存工作流配置')}
        </Button>
      </div>
    </div>
  );

  // ====== 模型广场管理 模型列表 Tab ======
  // 提取供应商和类型的唯一列表，用于筛选下拉
  const mpProviderOptions = Array.from(new Set(mpModels.map(m => m.provider_name).filter(Boolean)));
  const mpTypeOptions = Array.from(new Set(mpModels.map(m => m.type_name).filter(Boolean)));

  const filteredMpModels = mpModels.filter(m => {
    if (mpSearchKeyword.trim()) {
      const kw = mpSearchKeyword.trim().toLowerCase();
      if (!modelMatchesKeyword(m, mpSearchKeyword) && !(m.provider_name || '').toLowerCase().includes(kw)) {
        return false;
      }
    }
    // 供应商筛选
    if (mpProviderFilter !== 'all' && (m.provider_name || '') !== mpProviderFilter) return false;
    // 类型筛选
    if (mpTypeFilter !== 'all' && (m.type_name || '') !== mpTypeFilter) return false;
    // 展示状态筛选
    if (mpStatusFilter === 'enabled' && !m.mp_enabled) return false;
    if (mpStatusFilter === 'disabled' && m.mp_enabled) return false;
    // 模型状态筛选（模型管理中的启用/禁用状态）
    if (mpModelActiveFilter === 'active' && Number(m.is_active) !== 1) return false;
    if (mpModelActiveFilter === 'inactive' && Number(m.is_active) === 1) return false;
    return true;
  });

  // 一键切换当前筛选结果的广场展示状态
  const handleMpBatchToggle = async (enabled: boolean) => {
    const targetIds = filteredMpModels.map(m => m.id);
    if (targetIds.length === 0) return;
    const prevModels = mpModels;
    const prevPgModels = pgModels;
    const filteredIds = new Set(targetIds.map(id => String(id)));
    setMpModels(prev => prev.map(m => filteredIds.has(String(m.id)) ? { ...m, mp_enabled: enabled, mp_level_ids: [] } : m));
    setPgModels(prev => prev.map(m => filteredIds.has(String(m.id)) ? { ...m, mp_enabled: enabled, mp_level_ids: [] } : m));
    try {
      await request.post(`/plugins/${name}/marketplace-models/batch-toggle`, {
        ids: targetIds,
        enabled,
      });
      invalidateCoalesce('pg2026:public-config-and-user-configs');
      message.success(enabled ? '已批量开启广场展示' : '已批量关闭广场展示');
    } catch (e) {
      console.error(e);
      message.error('批量切换广场展示状态失败');
      setMpModels(prevModels);
      setPgModels(prevPgModels);
    }
  };

  const mpModelColumns = [
    {
      title: '模型名称',
      dataIndex: 'name',
      key: 'name',
      render: (nameVal: string, record: any) => {
        const scheme = pgSchemes.find(s => s.id === record.pg_scheme_id);
        return (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', lineHeight: 1.2 }}>
              <span
                style={{
                  fontWeight: 600,
                  color: record.is_active !== 1 ? (_isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)') : (_isLight ? '#1f2937' : '#fff'),
                  fontSize: 13,
                  lineHeight: 1.2,
                  wordBreak: 'break-all'
                }}
                title={nameVal}
              >
                {nameVal}
              </span>
              {record.is_active !== 1 && (
                <Tooltip title="该模型在模型管理中已被禁用，即使开启广场展示也不会在模型广场中显示">
                  <Tag color="error" style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px', margin: 0, borderRadius: 4 }}>已禁用</Tag>
                </Tooltip>
              )}
              {scheme && (
                <span style={{ fontSize: 11, color: '#1677ff', fontWeight: 'normal', lineHeight: 1.2 }}>
                  (已挂载流：{scheme.name})
                </span>
              )}
            </div>
            {record.remark && (
              <div
                style={{
                  fontSize: 12,
                  lineHeight: 1.2,
                  color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap'
                }}
                title={record.remark}
              >
                {record.remark}
              </div>
            )}
            <div
              style={{
                fontSize: 11,
                lineHeight: 1.2,
                color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)',
                fontFamily: 'monospace',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap'
              }}
              title={`MID: ${record.mid} | ${record.model_id}`}
            >
              MID: {record.mid} | {record.model_id}
            </div>
          </div>
        );
      }
    },
    {
      title: '供应商',
      dataIndex: 'provider_name',
      key: 'provider_name',
      width: 100,
      render: (p: string) => p ? (
        <Tag style={{ borderRadius: 4, background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.04)', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{p}</Tag>
      ) : <Text type="secondary">-</Text>
    },
    {
      title: '类型',
      dataIndex: 'type_name',
      key: 'type_name',
      width: 100,
      render: (t: string) => t ? (
        <Tag style={{ borderRadius: 4, background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.04)', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>
          {t.includes('封装') ? <ThunderboltOutlined style={{ marginRight: 4 }} /> : t.includes('视频') ? <VideoCameraOutlined style={{ marginRight: 4 }} /> : t.includes('图片') ? <PictureOutlined style={{ marginRight: 4 }} /> : null}
          {t}
        </Tag>
      ) : <Text type="secondary">-</Text>
    },
    {
      title: '模型状态',
      key: 'model_status',
      width: 80,
      render: (_: any, record: any) => (
        Number(record.is_active) === 1
          ? <Tag color="success" style={{ borderRadius: 4, margin: 0 }}>启用</Tag>
          : <Tooltip title="模型在模型管理中被禁用，不会出现在模型广场"><Tag color="error" style={{ borderRadius: 4, margin: 0 }}>禁用</Tag></Tooltip>
      )
    },
    {
      title: (
        <Tooltip title="开关开启时选择不可查看的用户等级，含未登录游客（默认全部可查看，勾选游客后即使基本配置允许免登录也看不到该模型）；开关关闭时选择可查看的用户等级（默认全部不可查看）。切换展示开关会清空已选等级。">
          <span>模型广场展示</span>
        </Tooltip>
      ),
      key: 'mp_level_ids',
      width: 240,
      render: (_: any, record: any) => (
        <MpLevelSelectCell
          record={record}
          levels={levels}
          _isLight={_isLight}
          showInactiveWarning
          onToggle={handleMpToggle}
          onChange={handleMpLevelIdsChange}
        />
      )
    },
    {
      title: (
        <Tooltip title="与模型列表「页面排序」同步，数值越大越靠前">
          <span>页面排序</span>
        </Tooltip>
      ),
      key: 'mp_sort_order',
      width: 120,
      sorter: (a: any, b: any) => (Number(a.mp_sort_order) || 0) - (Number(b.mp_sort_order) || 0),
      render: (_: any, record: any) => (
        <InputNumber
          size="small"
          min={0}
          value={Number(record.mp_sort_order) || 0}
          onChange={(val) => handleMpSortChange(record.id, Number(val) || 0)}
          style={{ width: 80 }}
        />
      )
    },
    {
      title: '广场描述',
      key: 'mp_description',
      width: 300,
      render: (_: any, record: any) => (
        <Space direction="vertical" size={4} style={{ width: '100%' }}>
          <Input
            size="small"
            value={record.mp_description || ''}
            onChange={(e) => handleMpDescChange(record.id, e.target.value, 'zh')}
            placeholder="中文简短描述..."
          />
          <Input
            size="small"
            value={record.mp_description_en || ''}
            onChange={(e) => handleMpDescChange(record.id, e.target.value, 'en')}
            placeholder="English short description..."
          />
        </Space>
      )
    },
  ];

  const marketplaceModelTab = (() => {
    const enabledCount = mpModels.filter(m => m.mp_enabled).length;
    const filteredEnabledCount = filteredMpModels.filter(m => m.mp_enabled).length;
    const allFilteredEnabled = filteredMpModels.length > 0 && filteredEnabledCount === filteredMpModels.length;

    return (
      <div>
        <div style={{ background: _isLight ? '#fff' : '#141414', borderRadius: 8, padding: '20px', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)' }}>
          {/* 标题 + 一键开关 */}
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 }}>
            <Text strong style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 14 }}>模型广场模型列表</Text>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)', fontSize: 12, whiteSpace: 'nowrap' }}>
                {filteredMpModels.length === mpModels.length ? '一键全部' : `当前 ${filteredMpModels.length} 项`}
              </Text>
              <Switch
                checked={allFilteredEnabled}
                onChange={(val) => handleMpBatchToggle(val)}
              />
            </div>
          </div>

          {/* 模型状态提示 */}
          {mpModels.some(m => Number(m.is_active) !== 1 && m.mp_enabled) && (
            <Alert
              type="warning"
              showIcon
              style={{ marginBottom: 16, borderRadius: 8 }}
              message={`有 ${mpModels.filter(m => Number(m.is_active) !== 1 && m.mp_enabled).length} 个模型已开启广场展示但在模型管理中处于禁用状态，这些模型不会在模型广场中显示。`}
            />
          )}
          <div style={{
            display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap',
            marginBottom: 16, padding: '10px 14px', borderRadius: 8,
            background: mpDisplayMode === 'blacklist'
              ? (_isLight ? 'rgba(22,119,255,0.04)' : 'rgba(22,119,255,0.08)')
              : (_isLight ? '#fafafa' : 'rgba(255,255,255,0.02)'),
            border: mpDisplayMode === 'blacklist'
              ? '1px solid rgba(22,119,255,0.2)'
              : (_isLight ? '1px solid rgba(0,0,0,0.04)' : '1px solid rgba(255,255,255,0.04)'),
          }}>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)', fontSize: 12, whiteSpace: 'nowrap' }}>展示模式：</Text>
            <Segmented
              options={[
                { label: '手动选择展示', value: 'whitelist' },
                { label: '默认全部展示', value: 'blacklist' },
              ]}
              value={mpDisplayMode}
              onChange={(val) => setMpDisplayMode(val as 'whitelist' | 'blacklist')}
              size="small"
            />
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>
              {mpDisplayMode === 'blacklist'
                ? '所有模型默认在广场展示（含新添加的），仅需关闭不想展示的。'
                : '模型默认不展示，需手动逐个开启。'}
            </Text>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12, marginLeft: 'auto' }}>
              已展示 {enabledCount} / {mpModels.length}（启用 {mpModels.filter(m => Number(m.is_active) === 1).length} 个）
            </Text>
          </div>

          {/* 筛选区 */}
          <div style={{
            display: 'flex', flexDirection: 'column', gap: 16,
            marginBottom: 16, padding: '16px', borderRadius: 8,
            background: _isLight ? '#fafafa' : 'rgba(255,255,255,0.02)',
            border: _isLight ? '1px solid rgba(0,0,0,0.04)' : '1px solid rgba(255,255,255,0.04)',
          }}>
            {/* 第一行：搜索 + 供应商 */}
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 24, flexWrap: 'wrap' }}>
              <Input
                placeholder="搜索模型名称 / ID..."
                value={mpSearchKeyword}
                onChange={e => setMpSearchKeyword(e.target.value)}
                style={{ width: 260 }}
                allowClear
                size="small"
              />
              {/* 模型品牌筛选 */}
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12, flex: 1 }}>
                <Text type="secondary" style={{ fontSize: 13, flexShrink: 0, marginTop: 2 }}>模型品牌:</Text>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                  <span
                    onClick={() => setMpProviderFilter('all')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpProviderFilter === 'all' ? '#1677ff' : 'transparent', color: mpProviderFilter === 'all' ? '#fff' : 'var(--text-secondary)' }}
                  >全部</span>
                  {mpProviderOptions.map(p => (
                    <span
                      key={p} onClick={() => setMpProviderFilter(p)}
                      style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpProviderFilter === p ? '#1677ff' : 'transparent', color: mpProviderFilter === p ? '#fff' : 'var(--text-secondary)' }}
                    >{p}</span>
                  ))}
                </div>
              </div>
            </div>

            {/* 第二行：类型 + 状态 */}
            <div style={{ display: 'flex', alignItems: 'flex-start', gap: 32, flexWrap: 'wrap' }}>
              {/* 类型筛选 */}
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
                <Text type="secondary" style={{ fontSize: 13, flexShrink: 0, marginTop: 2 }}>类型:</Text>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                  <span
                    onClick={() => setMpTypeFilter('all')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpTypeFilter === 'all' ? '#1677ff' : 'transparent', color: mpTypeFilter === 'all' ? '#fff' : 'var(--text-secondary)' }}
                  >全部</span>
                  {mpTypeOptions.map(t => (
                    <span
                      key={t} onClick={() => setMpTypeFilter(t)}
                      style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpTypeFilter === t ? '#1677ff' : 'transparent', color: mpTypeFilter === t ? '#fff' : 'var(--text-secondary)' }}
                    >{t}</span>
                  ))}
                </div>
              </div>

              {/* 广场展示状态筛选 */}
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
                <Text type="secondary" style={{ fontSize: 13, flexShrink: 0, marginTop: 2 }}>广场:</Text>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                  <span
                    onClick={() => setMpStatusFilter('all')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpStatusFilter === 'all' ? '#1677ff' : 'transparent', color: mpStatusFilter === 'all' ? '#fff' : 'var(--text-secondary)' }}
                  >全部</span>
                  <span
                    onClick={() => setMpStatusFilter('enabled')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpStatusFilter === 'enabled' ? '#1677ff' : 'transparent', color: mpStatusFilter === 'enabled' ? '#fff' : 'var(--text-secondary)' }}
                  >已展示</span>
                  <span
                    onClick={() => setMpStatusFilter('disabled')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpStatusFilter === 'disabled' ? '#1677ff' : 'transparent', color: mpStatusFilter === 'disabled' ? '#fff' : 'var(--text-secondary)' }}
                  >未展示</span>
                </div>
              </div>

              {/* 模型状态筛选（模型管理中的启用/禁用） */}
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
                <Text type="secondary" style={{ fontSize: 13, flexShrink: 0, marginTop: 2 }}>模型:</Text>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                  <span
                    onClick={() => setMpModelActiveFilter('all')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpModelActiveFilter === 'all' ? '#1677ff' : 'transparent', color: mpModelActiveFilter === 'all' ? '#fff' : 'var(--text-secondary)' }}
                  >全部</span>
                  <span
                    onClick={() => setMpModelActiveFilter('active')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpModelActiveFilter === 'active' ? '#52c41a' : 'transparent', color: mpModelActiveFilter === 'active' ? '#fff' : 'var(--text-secondary)' }}
                  >已启用</span>
                  <span
                    onClick={() => setMpModelActiveFilter('inactive')}
                    style={{ fontSize: 12, cursor: 'pointer', padding: '2px 10px', borderRadius: 12, background: mpModelActiveFilter === 'inactive' ? '#ff4d4f' : 'transparent', color: mpModelActiveFilter === 'inactive' ? '#fff' : 'var(--text-secondary)' }}
                  >已禁用</span>
                </div>
              </div>
            </div>
          </div>

          <Table
            dataSource={filteredMpModels}
            columns={mpModelColumns}
            rowKey="id"
            size="small"
            scroll={{ x: 1400 }}
            pagination={listPagination({
              current: mpPage,
              pageSize: mpPageSize,
              onChange: onMpPageChange,
            })}
            style={{ marginBottom: 16 }}
          />

          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Button type="primary" loading={savingMarketplace} onClick={handleSaveMarketplaceConfig} icon={<SaveOutlined />}>
              保存全部配置
            </Button>
          </div>
        </div>
      </div>
    );
  })();


  const currentDyn = plugin ? dynamicPlugins[plugin.name] : undefined;

  return (

    <div>
      {/* 页头 */}
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        marginBottom: 4,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <Button type="text" icon={<ArrowLeftOutlined />} onClick={() => navigate(`/${adminPath}/plugins`)} style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', padding: '4px 8px' }} />
          <div style={{
            width: 36, height: 36, borderRadius: 8, background: 'rgba(22,119,255,0.1)',
            display: 'flex', alignItems: 'center', justifyContent: 'center', color: '#1677ff'
          }}>
            {pluginIcons[plugin.name] || <AppstoreOutlined style={{ fontSize: 20 }} />}
          </div>
          <div>
            <Title level={4} style={{ margin: 0, color: _isLight ? '#1f2937' : '#fff', lineHeight: 1.3 }}>{t(`plugin_titles.${plugin.name}`, plugin.title)}</Title>
            <Text style={{ color: _isLight ? 'rgba(0,0,0,0.3)' : 'rgba(255,255,255,0.3)', fontSize: 12 }}>{plugin.name}</Text>
          </div>
        </div>
        {(plugin.name === 'site_portal' || plugin.name === 'site_portal_pro') && (
          <div>
            <Button
              icon={<EyeOutlined />}
              onClick={() => window.open(plugin.name === 'site_portal_pro' ? '/home-pro' : '/home', '_blank')}
            >
              预览门户
            </Button>
          </div>
        )}
      </div>



      {/* Tabs */}
      <Tabs
        activeKey={activeTabKey}
        onChange={handleTabChange}
        onTabClick={handleTabClick}
        tabBarStyle={{ marginBottom: name === 'playground_2026' && activeTabKey === 'playground_schemes' ? 4 : 12 }}
        items={
          currentDyn
            ? [
              { key: 'basic', label: '基本配置', children: basicTab },
              ...(currentDyn.tabs
                ? currentDyn.tabs.map((t: any) => ({
                    key: t.key,
                    label: t.label,
                    // 仅激活时挂载：离开 tab 卸载面板，停掉轮询/定时请求
                    children: activeTabKey === t.key ? (
                      <PluginModule>
                        {React.createElement(cachedLazy(`${name}:${t.key}`, t.component))}
                      </PluginModule>
                    ) : null,
                  }))
                : currentDyn.component
                  ? [
                      {
                        key: 'plugin_panel',
                        label: currentDyn.title,
                        children: activeTabKey === 'plugin_panel' ? (
                          <PluginModule>
                            {React.createElement(cachedLazy(`${name}:plugin_panel`, currentDyn.component!))}
                          </PluginModule>
                        ) : null,
                      }
                    ]
                  : []),
              // 独立 TOS 表单（可单独配置；未填则运行时回退全局存储）
              ...(name === 'upstream_asset_relay'
                ? [{ key: 'storage', label: '存储配置', children: storageTab }]
                : []),
            ]
            : plugin.name === 'high_availability_channel'
              ? [
              { key: 'basic', label: '基本配置', children: basicTab },
              { key: 'ha_config', label: '高可用参数配置', children: haConfigTab },
              { key: 'ha_logs', label: '使用日志记录', children: <PluginModule><HaLogs /></PluginModule> },
            ]
            : plugin.name === 'team_marketing'
              ? [
                { key: 'basic', label: '基本配置', children: basicTab },
                { key: 'team_config', label: '团队配置', children: <PluginModule><TeamConfig /></PluginModule> },
                { key: 'theme_promo', label: '主题推广', children: <PluginModule><ThemePromo /></PluginModule> },
              ]
            : plugin.name === 'playground_2026'
              ? [
                { key: 'basic', label: pg2026T('admin_tab_basic', '基本配置'), children: basicTab },
                { key: 'pg_storage', label: pg2026T('admin_tab_storage', '存储配置'), children: storageTab },
                { key: 'playground_models', label: pg2026T('admin_tab_models', '创作模型管理'), children: playgroundModelTab },
                { key: 'playground_schemes', label: pg2026T('admin_tab_schemes', '创作方案配置'), children: playgroundSchemeTab },
                { key: 'playground_chat_config', label: pg2026T('admin_tab_chat', '聊天配置'), children: (
                  <PluginModule>
                    <PlaygroundChatConfigTab
                      items={userNavModules}
                      onChange={setUserNavModules}
                      isLight={_isLight}
                    />
                  </PluginModule>
                ) },
                { key: 'playground_workflow_config', label: pg2026T('admin_tab_workflow', '工作流配置'), children: playgroundWorkflowConfigTab },
                { key: 'playground_image_edit', label: pg2026T('admin_image_edit', '图片编辑'), children: <PluginModule><PlaygroundImageEditConfigTab isLight={_isLight} /></PluginModule> },
                { key: 'playground_video_edit', label: pg2026T('admin_video_workbench', '视频编辑'), children: <PluginModule><PlaygroundVideoEditConfigTab isLight={_isLight} /></PluginModule> },
                { key: 'playground_skill_config', label: pg2026T('admin_tab_skill', 'Skill 配置'), children: <PluginModule><PlaygroundSkillConfigTab /></PluginModule> },
                { key: 'playground_prompt_optimize', label: pg2026T('admin_tab_prompt_optimize', 'AI 优化提示词'), children: <PluginModule><PlaygroundPromptOptimizeConfigTab isLight={_isLight} /></PluginModule> },
                { key: 'playground_error_mappings', label: pg2026T('admin_tab_error_mappings', '异常映射'), children: <PluginModule><PlaygroundErrorMappingConfigTab isLight={_isLight} /></PluginModule> },
              ]
            : plugin.name === 'playground'
              ? [
                { key: 'basic', label: '基本配置', children: basicTab },
                { key: 'pg_storage', label: '存储配置', children: storageTab },
                { key: 'playground_models', label: '创作模型管理', children: playgroundModelTab },
                { key: 'playground_schemes', label: '创作方案配置', children: playgroundSchemeTab },
                { key: 'playground_advanced_nodes', label: '高级节点配置', children: playgroundAdvancedNodesTab },
                { key: 'playground_agent_config', label: 'AI智能体配置', children: playgroundAgentConfigTab },
              ]
                  : plugin.name === 'model_marketplace'
                    ? [
                      { key: 'basic', label: '基本配置', children: basicTab },
                      { key: 'marketplace_models', label: '模型列表', children: marketplaceModelTab },
                      { key: 'marketplace_trending', label: '热门推荐', children: <PluginModule><MarketplaceTrendingTab config={mpTrendingConfig} onChange={setMpTrendingConfig} onSave={handleSaveMarketplaceConfig} saving={savingMarketplace} allModels={mpModels} allProviders={pgProvidersStats} /></PluginModule> },
                    ]
                    : plugin.name === 'site_icons'
                      ? [
                        { key: 'basic', label: '基本配置', children: basicTab },
                        { key: 'icon_library', label: '图标库管理', children: <PluginModule><SiteIconsManager /></PluginModule> },
                      ]
                        : plugin.name === 'site_portal'
                          ? [
                            { key: 'portal_manager', label: '门户管理', children: <PluginModule><PortalManager /></PluginModule> },
                            { key: 'style_selection', label: '风格选择', children: <PluginModule><PortalStyleSelection /></PluginModule> },
                            { key: 'basic', label: '基本配置', children: basicTab },
                            { key: 'storage', label: '门户存储配置', children: storageTab },
                          ]
                        : plugin.name === 'site_portal_pro'
                          ? [
                            { key: 'portal_manager', label: '门户管理', children: <PluginModule><PortalManagerPro /></PluginModule> },
                            {
                              key: 'article_manager',
                              label: '栏目文章',
                              children: activeTabKey === 'article_manager' ? (
                                <PluginModule>
                                  <PortalArticleManagerPro key={articleTabResetKey} />
                                </PluginModule>
                              ) : null,
                            },
                            { key: 'docs_manager', label: 'DOCS文档', children: <PluginModule><PortalDocsManager /></PluginModule> },
                            { key: 'style_selection', label: '风格选择', children: <PluginModule><PortalStyleSelectionPro /></PluginModule> },
                            { key: 'basic', label: '基本配置', children: basicTab },
                            { key: 'storage', label: '门户存储配置', children: storageTab },
                          ]
                        : plugin.name === 'docs_api'
                          ? [
                            { key: 'docs_manager', label: '文档管理', children: <PluginModule><DocsManager /></PluginModule> },
                            { key: 'basic', label: '基本配置', children: basicTab },
                          ]
                        : (plugin.name === 'asset_manager' || plugin.name === 'asset_manager_intl')
                            ? [
                              { key: 'basic', label: '基本配置', children: basicTab },
                              { key: 'api_access', label: 'API 接口调用', children: (
                                <ApiAccessConfig
                                  key={`${plugin.name}-${plugin.allowed_levels || 'all'}`}
                                  pluginNs={plugin.name}
                                  levels={levels}
                                  allowedLevels={plugin.allowed_levels || 'all'}
                                />
                              )},
                              { key: 'storage', label: '存储配置', children: storageTab },
                              { key: 'moderation', label: '审核配置', children: moderationTab },
                              {
                                key: 'moderation_query',
                                label: '风控查询',
                                // 仅激活时挂载，避免 Tabs 保活导致抢焦点/多余渲染
                                children: activeTabKey === 'moderation_query'
                                  ? <ModerationQuery key={plugin.name} pluginNs={plugin.name} />
                                  : null,
                              },
                              { key: 'audit_log', label: '审核日志', children: auditLogTab },
                              { key: 'preset', label: '预设素材', children: <AdminPresetAssets pluginNs={plugin.name} /> },
                              { key: 'relay_convert', label: '转换素材', children: <RelayConvertAssets pluginNs={plugin.name} /> },
                              { key: 'api_proxy', label: 'API 素材', children: <ApiProxyAssets pluginNs={plugin.name} /> },
                              { key: 'cloud_assets', label: '云端素材', children: <CloudAssetsTab pluginNs={plugin.name} /> },
                              { key: 'api_log', label: '接口日志', children: apiLogTab },
                            ]
                            : [
                              { key: 'basic', label: '基本配置', children: basicTab },
                            ]
        }
      />

      {/* 针对部分插件可能不挂载而导致 useForm() 失去关联的警告处理 */}
      {isPlaygroundLike && (
        <div style={{ display: 'none' }}>
          <Form form={moderationForm} />
        </div>
      )}
    </div>
  );
};

const PluginConfig: React.FC = () => (
  <App style={{ height: '100%', width: '100%' }}>
    <AppMessageBridge>
      <PluginConfigInner />
    </AppMessageBridge>
  </App>
);

export default PluginConfig;
