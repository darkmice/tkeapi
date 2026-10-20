/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useMemo, useRef } from 'react';
import { Card, Table, Button, Space, Form, Input, Switch, message, Popconfirm, Tag, Radio, InputNumber, Row, Col, Typography, Grid, Tooltip, Select, Modal, TimePicker, Alert, Segmented } from 'antd';
import MobileCardList, { MobileCard, CardRow, CardActions } from '../../components/MobileCardList';
import { listPagination, useListPager } from '../../components/ListPagination';
import { PlusOutlined, EditOutlined, DeleteOutlined, DeleteTwoTone, CopyOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import RateDisplay from './RateDisplay';
import TimeMultiplierChart from './TimeMultiplierChart';
import { useThemeStore } from '../../store/theme';
import { formDefaultFreeImageCount, resolveFreeImageCount } from '../../utils/billingFreeImages';
import {
  ALL_WEEKDAYS,
  expandTimeMultiplierIntervals,
  findOverlappingAllDayDays,
  findOverlappingTimeMultipliers,
  formatWeekdayRange,
  hasExplicitRemainder,
  invertRemainderFromExt,
  isAllDayMultiplier,
  isAllWeekdays,
  isFreeTimeMultiplier,
  migrateLegacyInvertItems,
  normalizeWeekdays,
  resolveTimeMultiplierAt,
} from '../../utils/timeMultipliers';
import { formatWallClock } from '../../utils/timedisplay';
import { convertScheduleSlots, scheduleInputTzName, type ScheduleTzMode } from './scheduleTzConvert';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import timezone from 'dayjs/plugin/timezone';

dayjs.extend(utc);
dayjs.extend(timezone);
import { type ModelProvider, type ModelType } from '../../types';
import ClassificationFilter from '../../components/Models/ClassificationFilter';
import { unclassifiedChip } from '../../utils/classificationParams';
import { fetchActivePlugins } from '../../utils/activePlugins';
import { billingRulesEditPath, billingRulesListPath, billingRulesNewPath } from './modelPaths';
const { Title, Text } = Typography;
const { useBreakpoint } = Grid;

const GPT_BILLING_ITEMS = [
  { key: 'input_text', label: '输入文本 (Text Input)' },
  { key: 'input_image', label: '输入图片 (Image Input)' },
  { key: 'output_image', label: '输出图片 (Image Output)' },
  { key: 'cached_input_text', label: '输入文本缓存 (Text Cached Input)' },
  { key: 'cached_input_image', label: '输入图片缓存 (Image Cached Input)' },
];

/** 使用 pricing_tiers 持久化阶梯配置的 billing_rule；未列入的规则保存时清空阶梯，避免脏数据串模式 */
const RULES_WITH_PRICING_TIERS = new Set([
  'tiered',
  'doubao_chat',
  'image_resolution',
  'image_resolution_io',
  'image_size_pixel',
  'video_resolution',
  'video_quality',
  'volc_seedream_pro',
  'minimax_h3',
  'video_seconds_io',
  'video_seconds_ref',
]);

const VIDU_VIDEO_ATTRS = [
  { value: 'text', label: '文生视频' },
  { value: 'image', label: '图生视频 (含首尾帧)' },
  { value: 'ref', label: '参考生视频 (主体/音频)' },
];
const VIDU_VIDEO_RESOLUTIONS = ['480p', '540p', '720p', '1080p', '2k', '4k'];

interface RuleContainerProps {
  isLight: boolean;
  title?: React.ReactNode;
  description?: React.ReactNode;
  extra?: React.ReactNode;
  children: React.ReactNode;
  padding?: string | number;
}

const RuleContainer: React.FC<RuleContainerProps> = ({ isLight, title, description, extra, children, padding = 20 }) => {
  return (
    <div style={{
      background: isLight ? '#fff' : '#141414',
      padding: typeof padding === 'number' ? `${padding}px` : padding,
      borderRadius: '12px',
      marginBottom: 24,
      border: isLight ? '1px solid #e8e8e8' : '1px solid #303030'
    }}>
      {(title || description || extra) && (
        <div style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: 12, marginBottom: description ? 6 : 0 }}>
            {title && (
              typeof title === 'string' ? (
                <Title level={5} style={{ margin: 0, fontSize: '14px', color: isLight ? 'rgba(0,0,0,0.85)' : 'rgba(255,255,255,0.85)' }}>
                  {title}
                </Title>
              ) : title
            )}
            {extra}
          </div>
          {description && (
            typeof description === 'string' ? (
              <Text type="secondary" style={{ fontSize: '12px', display: 'block', lineHeight: '1.5' }}>
                {description}
              </Text>
            ) : description
          )}
        </div>
      )}
      {children}
    </div>
  );
};

interface CacheRateControlProps {
  name: string;
  rateName: string;
  label: string;
  isLight: boolean;
}

const CacheRateControl: React.FC<CacheRateControlProps> = ({ name, rateName, label, isLight }) => {
  return (
    <Form.Item noStyle dependencies={[name]}>
      {({ getFieldValue: gfv }) => {
        const enabled = gfv(name);
        return (
          <div style={{ padding: '12px 16px', background: isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8, marginBottom: 12 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: enabled ? 12 : 0 }}>
              <Text style={{ fontSize: 13 }}>{label}</Text>
              <Form.Item name={name} valuePropName="checked" style={{ margin: 0 }}>
                <Switch size="small" />
              </Form.Item>
            </div>
            {enabled && (
              <Row gutter={16}>
                <Col span={12}>
                  <Form.Item name={rateName} style={{ marginBottom: 0 }}>
                    <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" placeholder={label} />
                  </Form.Item>
                </Col>
              </Row>
            )}
          </div>
        );
      }}
    </Form.Item>
  );
};

const WebSearchRateControl: React.FC<{ isLight: boolean; currencyUnit: string }> = ({ isLight, currencyUnit }) => {
  return (
    <div style={{ marginTop: 16, paddingTop: 16, borderTop: `1px dashed ${isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)'}` }}>
      <Row gutter={16}>
        <Col span={12}>
          <Form.Item name={["extended_config", "web_search_rate"]} label={`联网搜索单价 (Web Search, ${currencyUnit}/千次)`} initialValue={0}>
            <InputNumber style={{ width: '100%' }} precision={6} min={0} addonAfter="/ 千次" />
          </Form.Item>
        </Col>
      </Row>
    </div>
  );
};

interface PriceTableRowProps {
  label: string;
  value: number;
  isDisabled: boolean;
  precision?: number;
  step?: number;
  addonAfter: string;
  onChange: (val: number) => void;
  onActiveChange: (active: boolean) => void;
}

const PriceTableRow: React.FC<PriceTableRowProps> = ({
  label,
  value,
  isDisabled,
  precision = 6,
  step = 0.001,
  addonAfter,
  onChange,
  onActiveChange
}) => {
  return (
    <Row gutter={12} align="middle" style={{ marginBottom: 8, opacity: isDisabled ? 0.45 : 1 }}>
      <Col span={9}>
        <Text style={{ fontSize: 13 }}>{label}</Text>
      </Col>
      <Col span={10}>
        <InputNumber
          style={{ width: '100%' }}
          precision={precision}
          step={step}
          min={0}
          addonAfter={addonAfter}
          disabled={isDisabled}
          value={value}
          onChange={(val) => onChange(val ?? 0)}
        />
      </Col>
      <Col span={5} style={{ textAlign: 'right' }}>
        <Switch
          size="small"
          checked={!isDisabled}
          onChange={onActiveChange}
        />
      </Col>
    </Row>
  );
};

interface BillingRuleData {
  id: number;
  name: string;
  billing_type: string;
  billing_rule: string;
  prompt_rate: number;
  completion_rate: number;
  cached_rate?: number;
  claude_cache_creation_rate?: number;
  claude_cache_read_rate?: number;
  fixed_rate: number;
  duration_rate: number;
  pricing_tiers: string;
  extended_config: string;
  pid?: string;
  provider_id?: number;
  type_id?: number;
  pricing_type: string;
  is_active: number;
  is_system: number;
  sort_order?: number;
  created_at: string;
  updated_at: string;
}

const WEEKDAY_CHIPS = [
  { v: 1, l: '一' },
  { v: 2, l: '二' },
  { v: 3, l: '三' },
  { v: 4, l: '四' },
  { v: 5, l: '五' },
  { v: 6, l: '六' },
  { v: 7, l: '日' },
];
const WEEKDAY_FULL = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];

const WeekdayPicker: React.FC<{
  value?: number[];
  onChange?: (v: number[]) => void;
}> = ({ value, onChange }) => {
  const selected = normalizeWeekdays(value);
  const toggle = (day: number) => {
    const next = selected.includes(day)
      ? selected.filter((d) => d !== day)
      : [...selected, day].sort((a, b) => a - b);
    if (next.length === 0) return;
    onChange?.(next);
  };
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
      {WEEKDAY_CHIPS.map((chip) => {
        const checked = selected.includes(chip.v);
        return (
          <Tag.CheckableTag
            key={chip.v}
            checked={checked}
            onChange={() => toggle(chip.v)}
            style={{
              border: '1px solid',
              borderColor: checked ? 'transparent' : 'var(--border-color, rgba(128,128,128,0.2))',
              padding: '0 8px',
              fontSize: 12,
              lineHeight: '22px',
              marginInlineEnd: 0,
            }}
          >
            {chip.l}
          </Tag.CheckableTag>
        );
      })}
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([...ALL_WEEKDAYS])}>每天</Button>
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([1, 2, 3, 4, 5])}>工作日</Button>
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([6, 7])}>周末</Button>
    </div>
  );
};

function multiplierPhrase(rate: number): string {
  return isFreeTimeMultiplier(rate) ? '免费' : `${rate.toFixed(2)} 倍`;
}

const MultiplierInput: React.FC<{
  value?: number | null;
  onChange?: (value: number | null) => void;
  placeholder?: string;
  style?: React.CSSProperties;
  addonBefore?: React.ReactNode;
}> = ({ value, onChange, placeholder, style, addonBefore }) => {
  const [editing, setEditing] = useState(false);
  const hasValue = value != null && (value as unknown) !== '';
  const numeric = hasValue ? Number(value) : NaN;
  const showFree = !editing && isFreeTimeMultiplier(numeric);
  return (
    <InputNumber
      value={value ?? undefined}
      onChange={(next) => {
        if (next == null || (next as unknown) === '') {
          onChange?.(null);
          return;
        }
        const n = Number(next);
        onChange?.(Number.isFinite(n) ? n : null);
      }}
      placeholder={placeholder}
      min={0}
      precision={2}
      step={0.1}
      style={style}
      addonBefore={addonBefore}
      addonAfter={showFree ? undefined : '倍'}
      onFocus={() => setEditing(true)}
      onBlur={() => setEditing(false)}
      formatter={(raw, info) => {
        if (info?.userTyping) return info.input;
        if (raw == null || (raw as unknown) === '') return '';
        const n = Number(raw);
        if (isFreeTimeMultiplier(n)) return editing ? '0' : '免费';
        return String(raw);
      }}
      parser={(raw) => {
        const str = String(raw ?? '');
        if (str.includes('免费')) return 0;
        const text = str.trim();
        if (text === '' || text === '.') return '' as unknown as number;
        const n = Number(text);
        return Number.isFinite(n) ? n : ('' as unknown as number);
      }}
    />
  );
};

const RealTimeClock: React.FC<{
  defaultTz: string;
  label?: string;
  enabled?: boolean;
  items?: any[];
  remainder?: number;
}> = ({ defaultTz, label, enabled, items, remainder }) => {
  const [time, setTime] = useState(dayjs());

  useEffect(() => {
    const timer = setInterval(() => {
      setTime(dayjs());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  let displayTime = time;
  try {
    if (defaultTz) {
      if (defaultTz.startsWith('UTC') || defaultTz.match(/^[+-]\d/)) {
        displayTime = time.utcOffset(defaultTz.replace('UTC', ''));
      } else {
        displayTime = time.tz(defaultTz);
      }
    }
  } catch (e) {
    // Ignore invalid timezone
  }

  const weekday = displayTime.day() === 0 ? 7 : displayTime.day();
  const minutes = displayTime.hour() * 60 + displayTime.minute();
  const currentRate = enabled
    ? resolveTimeMultiplierAt(items, true, weekday, minutes, remainder)
    : 1;

  return (
    <Text type="secondary" style={{ fontSize: 12 }}>
      {label || '运营时间'}: {formatWallClock(defaultTz || 'UTC')} · {defaultTz}
      {enabled ? (isFreeTimeMultiplier(currentRate) ? ' · 此刻 免费' : ` · 此刻 ${currentRate.toFixed(2)} 倍`) : ''}
    </Text>
  );
};

const BillingRules: React.FC = () => {
  const { themeMode } = useThemeStore();
  const _isLight = themeMode === 'light';
  const { t } = useTranslation();
  const { settings } = useSettingsStore();
  const adminPath = settings?.site?.admin_path || 'admin1688';
  const operationTz = settings?.site?.default_timezone?.trim() || 'Asia/Shanghai';
  const adminTz = settings?.site?.admin_display_timezone?.trim() || operationTz;
  const navigate = useNavigate();
  const location = useLocation();
  const { id: routeParamId } = useParams<{ id?: string }>();
  const [searchParams] = useSearchParams();
  const normalizedPath = location.pathname.replace(/\/$/, '');
  const isNewRoute = /\/billing-rules\/new$/.test(normalizedPath);
  const isEditRoute = /\/billing-rules\/edit\/[^/]+$/.test(normalizedPath);
  const routeEditId = isEditRoute ? routeParamId : undefined;
  const isEditorOpen = isNewRoute || isEditRoute;
  const currencySymbol = settings?.currency?.currency_symbol || '$';
  const currencyUnit = settings?.currency?.currency_unit || '元';

  const auxiliaryCurrencies = useMemo(() => {
    const list = settings?.currency?.auxiliary_currencies;
    return Array.isArray(list) ? list.filter(c => c.enabled) : [];
  }, [settings?.currency?.auxiliary_currencies]);
  const [selectedCurrencyCode, setSelectedCurrencyCode] = useState<string>('');

  const formatPrice = (price: number | string | undefined | null) => {
    if (price === undefined || price === null || price === '') return '-';
    const num = Number(price);
    if (isNaN(num)) return String(price);
    if (selectedCurrencyCode === '') {
      return `${currencySymbol}${num}`;
    }
    const curr = auxiliaryCurrencies.find(c => c.code === selectedCurrencyCode);
    if (curr) {
      return `${curr.symbol}${(num * curr.exchange_rate).toFixed(6).replace(/\.?0+$/, '')}`;
    }
    return `${currencySymbol}${num}`;
  };

  const [items, setItems] = useState<BillingRuleData[]>([]);
  const [filterType, setFilterType] = useState('all');
  const [filterPricingType, setFilterPricingType] = useState('all');
  const [searchText, setSearchText] = useState('');
  const [loading, setLoading] = useState(true);
  const { page: currentPage, pageSize, setPage: setCurrentPage, onChange } = useListPager();
  const [saving, setSaving] = useState(false);
  const [editingItem, setEditingItem] = useState<BillingRuleData | null>(null);
  const [copySource, setCopySource] = useState<BillingRuleData | null>(null);
  const [copyName, setCopyName] = useState('');
  const [copying, setCopying] = useState(false);
  const [billingType, setBillingType] = useState('tokens');
  const [form] = Form.useForm();
  const screens = useBreakpoint();
  // 精确查表模式：使用独立 state 管理价格表（form store 无法触发 re-render）
  const [klingPriceTable, setKlingPriceTable] = useState<Record<string, number>>({});
  const [klingDisabledKeys, setKlingDisabledKeys] = useState<string[]>([]);

  // 腾讯云 Vidu 视频/图片精确查表 state
  const [viduVideoPriceTable, setViduVideoPriceTable] = useState<Record<string, number>>({});
  const [viduVideoDisabledKeys, setViduVideoDisabledKeys] = useState<string[]>([]);
  const [viduActiveAttr, setViduActiveAttr] = useState<string>('text');
  const [viduImagePriceTable, setViduImagePriceTable] = useState<Record<string, number>>({});
  const [viduImageDisabledKeys, setViduImageDisabledKeys] = useState<string[]>([]);
  // 火山引擎级联增强精确查表 state
  const [volcCascadePriceTable, setVolcCascadePriceTable] = useState<Record<string, number>>({});
  const [volcCascadeDisabledKeys, setVolcCascadeDisabledKeys] = useState<string[]>([]);
  const [allProviders, setAllProviders] = useState<ModelProvider[]>([]);
  const [allTypes, setAllTypes] = useState<ModelType[]>([]);
  const [filterProvider, setFilterProvider] = useState<number | null>(null);
  const [filterTypeSelect, setFilterTypeSelect] = useState<number | null>(null);
  const [hasVolcengineEnhance, setHasVolcengineEnhance] = useState(false);
  const tableContainerRef = useRef<HTMLDivElement>(null);
  const newFormReadyRef = useRef(false);
  const hydratedEditIdRef = useRef<string | null>(null);

  useEffect(() => {
    setCurrentPage(1);
  }, [filterType, filterPricingType, searchText, filterProvider, filterTypeSelect]);

  const fetchPlugins = async () => {
    try {
      const res = await fetchActivePlugins();
      const active = res?.active_plugins || [];
      setHasVolcengineEnhance(active.some((p: any) => p.name === 'volcengine_enhance'));
    } catch (e) {
      console.error(e);
    }
  };

  const fetchClassifications = async () => {
    try {
      const providers = await (request.get('/model-providers') as any);
      setAllProviders(Array.isArray(providers) ? providers.filter(p => p.is_active) : []);
      const types = await (request.get('/model-types') as any);
      setAllTypes(Array.isArray(types) ? types.filter(t => t.is_active) : []);
    } catch (e) {
      console.error(e);
    }
  };

  const fetchItems = async () => {
    setLoading(true);
    try {
      const resp = await (request.get('/billing-rules', {
        params: routeEditId ? { include_pid: routeEditId } : undefined,
      }) as any);
      const list = Array.isArray(resp) ? resp : [];
      list.sort((a: BillingRuleData, b: BillingRuleData) => {
        const timeA = a.updated_at ? dayjs(a.updated_at).valueOf() : 0;
        const timeB = b.updated_at ? dayjs(b.updated_at).valueOf() : 0;
        if (timeB !== timeA) {
          return timeB - timeA;
        }
        return (b.id || 0) - (a.id || 0);
      });
      setItems(list);
    } catch (e) {
      console.error(e);
      setItems([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchClassifications();
    fetchPlugins();
  }, []);

  useEffect(() => {
    fetchItems();
  }, [routeEditId]);

  // ===== PC端鼠标左键按下拖拽滚动表格功能 =====
  useEffect(() => {
    const tableEl = tableContainerRef.current;
    if (!tableEl) return;

    // 查找 Ant Design Table 内部实际滚动的容器
    const container = tableEl.querySelector('.ant-table-content') || tableEl.querySelector('.ant-table-body') as HTMLElement | null;
    if (!container) return;

    let isDown = false;
    let startX: number;
    let scrollLeft: number;

    const handleMouseDown = (e: MouseEvent) => {
      // 避开可操作性子项（按钮、链接、输入框等），以及选中文本的操作
      const target = e.target as HTMLElement;
      if (
        target.closest('button') || 
        target.closest('a') || 
        target.closest('input') || 
        target.closest('textarea') ||
        target.closest('.ant-typography-copy') ||
        target.closest('.ant-select') ||
        target.closest('.ant-table-filter-trigger') ||
        target.closest('.ant-popconfirm') ||
        target.closest('.ant-popover') ||
        window.getSelection()?.toString()
      ) {
        return;
      }

      isDown = true;
      startX = e.pageX - (container as HTMLElement).offsetLeft;
      scrollLeft = (container as HTMLElement).scrollLeft;
    };

    const handleMouseLeave = () => {
      isDown = false;
    };

    const handleMouseUp = () => {
      isDown = false;
    };

    const handleMouseMove = (e: MouseEvent) => {
      if (!isDown) return;
      e.preventDefault();
      const x = e.pageX - (container as HTMLElement).offsetLeft;
      const walk = (x - startX) * 1.5; // 滚动速度倍率
      (container as HTMLElement).scrollLeft = scrollLeft - walk;
    };

    // 默认保持系统默认指针，不显示手掌手势
    (container as HTMLElement).style.cursor = 'default';

    container.addEventListener('mousedown', handleMouseDown as any);
    container.addEventListener('mouseleave', handleMouseLeave as any);
    container.addEventListener('mouseup', handleMouseUp as any);
    container.addEventListener('mousemove', handleMouseMove as any);

    return () => {
      (container as HTMLElement).style.cursor = '';
      container.removeEventListener('mousedown', handleMouseDown as any);
      container.removeEventListener('mouseleave', handleMouseLeave as any);
      container.removeEventListener('mouseup', handleMouseUp as any);
      container.removeEventListener('mousemove', handleMouseMove as any);
    };
  }, [loading]);

  const handleAdd = () => {
    navigate(billingRulesNewPath(adminPath));
  };

  const initNewBillingForm = () => {
    setEditingItem(null);
    setBillingType('tokens');
    setKlingPriceTable({});
    setKlingDisabledKeys([]);
    setViduVideoPriceTable({});
    setViduVideoDisabledKeys([]);
    setViduActiveAttr('text');
    setViduImagePriceTable({});
    setViduImageDisabledKeys([]);
    setVolcCascadePriceTable({});
    setVolcCascadeDisabledKeys([]);
    form.resetFields();
    form.setFieldsValue({
      pid: '',
      billing_type: 'tokens',
      billing_rule: 'standard',
      pricing_type: 'custom',
      is_active: true,
      sort_order: 0,
      prompt_rate: 0,
      completion_rate: 0,
      fixed_rate: 0,
      duration_rate: 0,
      pricing_tiers: [],
      extended_config: {
        enable_time_multipliers: false,
        invert_time_multipliers: false,
        invert_remainder_multiplier: 1,
        time_input_tz: 'operation',
        time_multipliers: [],
        web_search_rate: 0,
      },
      sd2_resolutions: [
        { resolution: '480p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '720p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '1080p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '4k', enabled: false, with_video: 0, without_video: 0 },
      ],
      volc_audio_rate: 0, volc_base_rate: 0,
      volc_offline_discount: 0.5,
      s1_online_rate: 0, s1_offline_rate: 0,
      prompt_extend_multiplier: 1,
      image_ref_multiplier: 1,
      quality_pricing_enabled: false,
      doubao_fast_enabled: false,
      layer_pricing_enabled: false,
      kling_mode_std: 1.0, kling_mode_pro: 1.33, kling_mode_2k: 1.5, kling_mode_4k: 2.0,
      kling_sound_off: 1.0, kling_sound_on: 1.5,
      kling_video_ref_no: 1.0, kling_video_ref_yes: 1.5,
      kling_enable_mode: true, kling_enable_sound: true, kling_enable_video_ref: false,
      kling_use_price_table: false,
      vidu_offpeak_discount: 1.0,
      enable_cached_rate: false,
      enable_claude_cache_creation: false,
      enable_claude_cache_read: false,
      provider_id: null,
      type_id: null,
      supported_models: [],
      free_ref_tokens: 4096,
      ref_token_rate_per_1k: 0.02,
      image_tokens_default: 1024,
      video_ref_tokens_480p: 2886,
      video_ref_tokens_768p: 7459,
      audio_ref_tokens_per_sec: 80,
      ...(() => {
        const gptDefaultValues: Record<string, any> = {};
        GPT_BILLING_ITEMS.forEach(it => {
          gptDefaultValues[`gpt_${it.key}_enabled`] = false;
          gptDefaultValues[`gpt_${it.key}_rate`] = 0;
        });
        return gptDefaultValues;
      })(),
    });
  };

    const hydrateBillingEditor = (item: BillingRuleData) => {
    let tiers: any[] = [];
    let ext: any = {};
    try {
      if (item.pricing_tiers) {
        const parsed = JSON.parse(item.pricing_tiers);
        if (Array.isArray(parsed)) {
          // image_size_pixel：旧表单曾误存 max_pixels，编辑时保留其余字段，需用户补全 size
          tiers.push(...parsed.map((t: any) => ({ ...t })));
        }
      }
      if (item.extended_config) {
        Object.assign(ext, JSON.parse(item.extended_config));
      }
    } catch (e) { }

    setEditingItem(item);
    setBillingType(item.billing_type);
    // 可灵精确查表回显
    if (item.billing_rule === 'kling_video') {
      setKlingPriceTable(ext.price_table || {});
      setKlingDisabledKeys(Array.isArray(ext.price_table_disabled) ? ext.price_table_disabled : []);
    } else {
      setKlingPriceTable({});
      setKlingDisabledKeys([]);
    }
    // Vidu 视频/图片精确查表回显
    if (item.billing_rule === 'vidu_video') {
      setViduVideoPriceTable(ext.price_table || {});
      setViduVideoDisabledKeys(Array.isArray(ext.price_table_disabled) ? ext.price_table_disabled : []);
    } else {
      setViduVideoPriceTable({});
      setViduVideoDisabledKeys([]);
    }
    if (item.billing_rule === 'vidu_image') {
      setViduImagePriceTable(ext.price_table || {});
      setViduImageDisabledKeys(Array.isArray(ext.price_table_disabled) ? ext.price_table_disabled : []);
    } else {
      setViduImagePriceTable({});
      setViduImageDisabledKeys([]);
    }
    if (item.billing_rule === 'volc_enhance_cascade') {
      setVolcCascadePriceTable(ext.price_table || {});
      setVolcCascadeDisabledKeys(Array.isArray(ext.price_table_disabled) ? ext.price_table_disabled : []);
    } else {
      setVolcCascadePriceTable({});
      setVolcCascadeDisabledKeys([]);
    }

    let timeMultipliers: any[] = [];
    const timeInputTz = ext.time_input_tz === 'admin_display' ? 'admin_display' : 'operation';
    if (Array.isArray(ext.time_multipliers)) {
      timeMultipliers = ext.time_multipliers.map((i: any) => ({
        start: i.start ? dayjs(i.start, 'HH:mm') : null,
        end: i.end ? dayjs(i.end, 'HH:mm') : null,
        multiplier: i.multiplier ?? 1.0,
        days: normalizeWeekdays(i.days),
        all_day: !!i.all_day,
      })).filter((i: any) => i.all_day || (i.start && i.end));
      if (ext.invert_time_multipliers && !hasExplicitRemainder(ext)) {
        timeMultipliers = migrateLegacyInvertItems(timeMultipliers);
      }
      timeMultipliers = convertScheduleSlots(
        timeMultipliers,
        operationTz,
        scheduleInputTzName(timeInputTz, operationTz, adminTz),
      );
    }

    let sd2_resolutions: any[] = [];
    if (ext.resolution_rates) {
      Object.keys(ext.resolution_rates).forEach(res => {
        sd2_resolutions.push({
          resolution: res,
          enabled: true,
          with_video: ext.resolution_rates[res].with_video || 0,
          without_video: ext.resolution_rates[res].without_video || 0,
        });
      });
    } else {
      sd2_resolutions = [
        { resolution: '480p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '720p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '1080p', enabled: false, with_video: 0, without_video: 0 },
        { resolution: '4k', enabled: false, with_video: 0, without_video: 0 },
      ];
    }

    form.setFieldsValue({
      ...item,
      pricing_tiers: tiers,
      sd2_resolutions,
      volc_audio_rate: ext.audio_rate || 0,
      volc_base_rate: ext.base_rate || 0,
      volc_offline_discount: ext.offline_discount ?? 0.5,
      s1_online_rate: ext.online_rate || 0,
      s1_offline_rate: ext.offline_rate || 0,
      image_prompt_rate: ext.image_prompt_rate || 0,
      image_completion_rate: ext.image_completion_rate || 0,
      prompt_extend_multiplier: ext.prompt_extend_multiplier || 1,
      image_ref_multiplier: ext.image_ref_multiplier ?? 1,
      free_image_count: resolveFreeImageCount(ext.free_image_count, item.billing_rule),
      free_ref_tokens: ext.free_ref_tokens ?? 4096,
      ref_token_rate_per_1k: ext.ref_token_rate_per_1k ?? 0.02,
      image_tokens_default: ext.image_tokens_default ?? 1024,
      video_ref_tokens_480p: ext.video_ref_tokens_per_sec?.['480p'] ?? 2886,
      video_ref_tokens_768p: ext.video_ref_tokens_per_sec?.['768p'] ?? 7459,
      audio_ref_tokens_per_sec: ext.audio_ref_tokens_per_sec ?? 80,
      quality_pricing_enabled: !!ext.quality_pricing_enabled
        || (item.billing_rule === 'image_size_pixel' && tiers.some((t: any) => t.quality_pricing)),
      doubao_fast_enabled: !!ext.doubao_fast_enabled,
      layer_pricing_enabled: !!ext.layer_pricing_enabled
        || (item.billing_rule === 'volc_seedream_pro' && tiers.some((t: any) => Number(t.layer_rate) > 0)),
      kling_mode_std: ext.mode_multipliers?.std ?? 1.0,
      kling_mode_pro: ext.mode_multipliers?.pro ?? 1.33,
      kling_mode_4k: ext.mode_multipliers?.['4k'] ?? 2.0,
      kling_mode_2k: ext.mode_multipliers?.['2k'] ?? 1.5,
      kling_sound_off: ext.sound_multipliers?.off ?? 1.0,
      kling_sound_on: ext.sound_multipliers?.on ?? 1.5,
      kling_video_ref_no: ext.video_ref_multipliers?.no ?? 1.0,
      kling_video_ref_yes: ext.video_ref_multipliers?.yes ?? 1.5,
      kling_enable_mode: ext.enable_mode !== false,
      kling_enable_sound: ext.enable_sound !== false,
      kling_enable_video_ref: ext.enable_video_ref === true,
      enable_video_ref: ext.enable_video_ref !== false,
      kling_use_price_table: item.billing_rule === 'kling_video' && !!ext.price_table && Object.keys(ext.price_table).length > 0,
      vidu_offpeak_discount: ext.offpeak_discount ?? 1.0,
      enable_cached_rate: (item as any).cached_rate > 0,
      enable_claude_cache_creation: (item as any).claude_cache_creation_rate > 0,
      enable_claude_cache_read: (item as any).claude_cache_read_rate > 0,
      is_active: item.is_active === 1,
      provider_id: item.provider_id ?? null,
      type_id: item.type_id ?? null,
      supported_models: Array.isArray(ext.supported_models) ? ext.supported_models : [],
      ...(() => {
        const gptConfig = ext.gpt_config || {};
        const gptValues: Record<string, any> = {};
        GPT_BILLING_ITEMS.forEach(it => {
          const cfg = gptConfig[it.key] || {};
          gptValues[`gpt_${it.key}_enabled`] = !!cfg.enabled;
          gptValues[`gpt_${it.key}_rate`] = cfg.rate ?? 0;
        });
        return gptValues;
      })(),
      extended_config: {
        enable_time_multipliers: !!ext.enable_time_multipliers,
        invert_time_multipliers: !!ext.invert_time_multipliers,
        invert_remainder_multiplier: invertRemainderFromExt(ext),
        time_input_tz: timeInputTz,
        time_multipliers: timeMultipliers,
        web_search_rate: ext.web_search_rate || 0,
      },
    });
  };

  const handleEdit = (item: BillingRuleData) => {
    navigate(billingRulesEditPath(adminPath, item.pid || item.id));
  };

  const handleCloseEditor = () => {
    navigate(billingRulesListPath(adminPath));
  };

  useEffect(() => {
    const legacyPid = searchParams.get('pid');
    const legacyId = searchParams.get('edit_id');
    const target = legacyPid || legacyId;
    if (target) {
      navigate(billingRulesEditPath(adminPath, target), { replace: true });
    }
  }, [searchParams, adminPath]);

  useEffect(() => {
    if (!isNewRoute) {
      newFormReadyRef.current = false;
      return;
    }
    if (newFormReadyRef.current) return;
    newFormReadyRef.current = true;
    initNewBillingForm();
  }, [isNewRoute]);

  useEffect(() => {
    if (!routeEditId) {
      // 新建页点「保存更新」会先短暂停在 /new 再跳编辑页；此时不要清掉刚写入的 id，否则列表尚未包含新规则时会被误踢回列表
      if (!isNewRoute) {
        hydratedEditIdRef.current = null;
      }
      return;
    }
    if (hydratedEditIdRef.current === routeEditId) return;
    if (loading) return;
    const target = items.find((item) => item.pid && String(item.pid) === String(routeEditId))
      || items.find((item) => String(item.id) === String(routeEditId));
    if (target) {
      hydratedEditIdRef.current = routeEditId;
      hydrateBillingEditor(target);
    } else if (editingItem && ((editingItem.pid && String(editingItem.pid) === String(routeEditId)) || String(editingItem.id) === String(routeEditId))) {
      hydratedEditIdRef.current = routeEditId;
    } else {
      message.error('未找到指定计费规则');
      handleCloseEditor();
    }
  }, [routeEditId, items, loading, isNewRoute, editingItem]);

  const handleDelete = async (id: number) => {
    try {
      await request.delete(`/billing-rules/${id}`);
      message.success(t('common.success'));
      fetchItems();
    } catch (e) {
      console.error(e);
    }
  };

  const openCopy = (record: BillingRuleData) => {
    setCopySource(record);
    setCopyName(`${record.name} 副本`);
  };

  const parseBillingJson = (raw: string | undefined, fallback: unknown) => {
    if (raw == null || raw === '') return fallback;
    try {
      return JSON.parse(raw);
    } catch {
      return fallback;
    }
  };

  const commitCopy = async () => {
    if (!copySource || copying) return;
    const nameTrim = (copyName || '').trim();
    if (!nameTrim) {
      message.error('请填写规则名称');
      return;
    }
    if (items.some(i => i.name === nameTrim)) {
      message.error('该费用规则名称已存在');
      return;
    }
    try {
      setCopying(true);
      await request.post('/billing-rules', {
        name: nameTrim,
        billing_type: copySource.billing_type,
        prompt_rate: copySource.prompt_rate || 0,
        completion_rate: copySource.completion_rate || 0,
        cached_rate: copySource.cached_rate || 0,
        claude_cache_creation_rate: copySource.claude_cache_creation_rate || 0,
        claude_cache_read_rate: copySource.claude_cache_read_rate || 0,
        fixed_rate: copySource.fixed_rate || 0,
        duration_rate: copySource.duration_rate || 0,
        billing_rule: copySource.billing_rule,
        pricing_tiers: parseBillingJson(copySource.pricing_tiers, []),
        extended_config: parseBillingJson(copySource.extended_config, {}),
        provider_id: copySource.provider_id || null,
        type_id: copySource.type_id || null,
        is_active: copySource.is_active ?? 1,
        pricing_type: copySource.pricing_type || 'custom',
        sort_order: copySource.sort_order || 0,
      });
      message.success('复制规则成功');
      setCopySource(null);
      fetchItems();
    } catch (e) {
      console.error(e);
    } finally {
      setCopying(false);
    }
  };

  const handleSave = async (values: any, stayOnPage = false) => {
    setSaving(true);
    try {
      // 提取并校验时间段倍率设置
      const enableTimeMultipliers = form.getFieldValue(['extended_config', 'enable_time_multipliers']) === true;
      let timeMultipliers = form.getFieldValue(['extended_config', 'time_multipliers']) || [];

      if (enableTimeMultipliers) {
        if (!timeMultipliers.length) {
          message.error("已开启时间段价格倍率，请至少添加一条时段或全天规则");
          return;
        }
        const intervals = [];
        for (let i = 0; i < timeMultipliers.length; i++) {
          const item = timeMultipliers[i];
          if (item.multiplier === undefined || item.multiplier === null || item.multiplier === '' || !Number.isFinite(Number(item.multiplier)) || Number(item.multiplier) < 0) {
            message.error(`第 ${i + 1} 个规则的倍率无效`);
            return;
          }

          const days = normalizeWeekdays(item.days);
          if (days.length === 0) {
            message.error(`第 ${i + 1} 个规则请至少选择一个星期`);
            return;
          }

          if (isAllDayMultiplier(item)) {
            continue;
          }

          if (!item.start || !item.end) {
            message.error("请完整填写所有时间段的起止时间");
            return;
          }
          const startMin = item.start.hour() * 60 + item.start.minute();
          const endMin = item.end.hour() * 60 + item.end.minute();

          if (startMin === endMin) {
            message.error(`第 ${i + 1} 个时间段的起止时间不能相同`);
            return;
          }

          intervals.push(...expandTimeMultiplierIntervals(startMin, endMin, days, i));
        }

        const overlap = findOverlappingTimeMultipliers(intervals);
        if (overlap) {
          const [intA, intB] = overlap;
          message.error(`配置的时间段在${WEEKDAY_FULL[intA.day - 1]}存在重叠，请修正（第 ${intA.index + 1} 组与第 ${intB.index + 1} 组）`);
          return;
        }
        const allDayOverlap = findOverlappingAllDayDays(timeMultipliers);
        if (allDayOverlap) {
          message.error(`${WEEKDAY_FULL[allDayOverlap.day - 1]}全天被第 ${allDayOverlap.a + 1} 组与第 ${allDayOverlap.b + 1} 组重复设置，请合并为一条`);
          return;
        }
      }
      if (!RULES_WITH_PRICING_TIERS.has(values.billing_rule)) {
        values.pricing_tiers = [];
      }

      // 只有特定多模态规则才打包 extended_config，其他保持空对象
      let extConfig: Record<string, any> = {};
      if (values.billing_rule === 'seedance2.0') {
        let resRates: any = {};
        if (values.sd2_resolutions && Array.isArray(values.sd2_resolutions)) {
          values.sd2_resolutions.forEach((item: any) => {
            const key = String(item.resolution || '').trim().toLowerCase();
            if (item.enabled && key) {
              resRates[key] = {
                with_video: item.with_video || 0,
                without_video: item.without_video || 0
              };
            }
          });
        }
        extConfig = { resolution_rates: resRates };
      } else if (values.billing_rule === 'seedance1.5pro') {
        extConfig = {
          audio_rate: values.volc_audio_rate || 0,
          base_rate: values.volc_base_rate || 0,
          offline_discount: values.volc_offline_discount ?? 0.5,
        };
      } else if (values.billing_rule === 'seedance1.0') {
        extConfig = {
          online_rate: values.s1_online_rate || 0,
          offline_rate: values.s1_offline_rate || 0,
        };
      } else if (values.billing_rule === 'kling_video') {
        // 直接从 form store 读取（这些字段通过条件渲染挂载，onFinish values 中可能缺失）
        const enableMode = form.getFieldValue('kling_enable_mode') !== false;
        const enableSound = form.getFieldValue('kling_enable_sound') !== false;
        const enableVideoRef = form.getFieldValue('kling_enable_video_ref') === true;
        const usePriceTable = form.getFieldValue('kling_use_price_table');
        if (usePriceTable && Object.keys(klingPriceTable).length > 0) {
          // 精确查表模式——从 React state 读取
          extConfig = {
            price_table: klingPriceTable,
            price_table_disabled: klingDisabledKeys,
            enable_mode: enableMode,
            enable_sound: enableSound,
            enable_video_ref: enableVideoRef,
          };
        } else {
          // 倍率模式（兼容旧规则）
          extConfig = {
            mode_multipliers: { std: values.kling_mode_std ?? 1.0, pro: values.kling_mode_pro ?? 1.33, '2k': values.kling_mode_2k ?? 1.5, '4k': values.kling_mode_4k ?? 2.0 },
            sound_multipliers: { off: values.kling_sound_off ?? 1.0, on: values.kling_sound_on ?? 1.5 },
            video_ref_multipliers: { no: values.kling_video_ref_no ?? 1.0, yes: values.kling_video_ref_yes ?? 1.5 },
            enable_mode: enableMode,
            enable_sound: enableSound,
            enable_video_ref: enableVideoRef,
          };
        }
      } else if (values.billing_rule === 'vidu_video') {
        extConfig = {
          price_table: viduVideoPriceTable,
          price_table_disabled: viduVideoDisabledKeys,
          offpeak_discount: values.vidu_offpeak_discount ?? 1.0,
        };
      } else if (values.billing_rule === 'volc_enhance_cascade') {
        extConfig = {
          price_table: volcCascadePriceTable,
          price_table_disabled: volcCascadeDisabledKeys,
        };
      } else if (values.billing_rule === 'vidu_image') {
        extConfig = {
          price_table: viduImagePriceTable,
          price_table_disabled: viduImageDisabledKeys,
        };
      } else if (values.billing_rule === 'multimodal') {
        extConfig = {
          image_prompt_rate: values.image_prompt_rate || 0,
          image_completion_rate: values.image_completion_rate || 0,
        };
        values.completion_rate = values.completion_rate || 0;
      } else if (values.billing_rule === 'gpt_billing') {
        const gpt_config: Record<string, any> = {};
        GPT_BILLING_ITEMS.forEach(it => {
          gpt_config[it.key] = {
            enabled: !!values[`gpt_${it.key}_enabled`],
            rate: values[`gpt_${it.key}_rate`] || 0,
          };
        });

        extConfig = {
          gpt_config,
        };
        values.prompt_rate = values.gpt_input_text_rate || 0;
        values.completion_rate = values.gpt_output_image_rate || 0;
        values.cached_rate = 0;
        values.fixed_rate = 0;
        values.duration_rate = 0;
        values.claude_cache_creation_rate = 0;
        values.claude_cache_read_rate = 0;
      }

      // 图像模型特有：提示词扩写倍率（per_image 全局生效，image_resolution 仅保留扩写倍率，有图倍率已下沉到每个阶梯行）
      if (values.billing_rule === 'per_image') {
        extConfig = { ...extConfig, prompt_extend_multiplier: values.prompt_extend_multiplier || 1, image_ref_multiplier: values.image_ref_multiplier ?? 1 };
      } else if (values.billing_rule === 'image_resolution') {
        extConfig = { ...extConfig, prompt_extend_multiplier: values.prompt_extend_multiplier || 1 };
      } else if (values.billing_rule === 'image_size_pixel') {
        const qpEnabled = form.getFieldValue('quality_pricing_enabled') === true;
        extConfig = {
          ...extConfig,
          prompt_extend_multiplier: values.prompt_extend_multiplier || 1,
          quality_pricing_enabled: qpEnabled,
        };
      } else if (values.billing_rule === 'doubao_chat') {
        const fastEnabled = form.getFieldValue('doubao_fast_enabled') === true;
        extConfig = { ...extConfig, doubao_fast_enabled: fastEnabled };
      } else if (values.billing_rule === 'minimax_h3' || values.billing_rule === 'video_seconds_io' || values.billing_rule === 'video_seconds_ref' || values.billing_rule === 'volc_seedream_pro') {
        const freeDefault = formDefaultFreeImageCount(values.billing_rule);
        const freeCount = Number(values.free_image_count);
        extConfig = {
          ...extConfig,
          free_image_count: Number.isFinite(freeCount) && freeCount >= 0 ? freeCount : freeDefault,
        };
        if (values.billing_rule === 'video_seconds_ref') {
          extConfig.enable_video_ref = form.getFieldValue('enable_video_ref') !== false;
        }
        if (values.billing_rule === 'volc_seedream_pro') {
          extConfig.layer_pricing_enabled = form.getFieldValue('layer_pricing_enabled') === true;
        }
      } else if (values.billing_rule === 'fal_ref_video') {
        extConfig = {
          ...extConfig,
          free_ref_tokens: Number(values.free_ref_tokens ?? 4096),
          ref_token_rate_per_1k: Number(values.ref_token_rate_per_1k ?? 0.02),
          image_tokens_default: Number(values.image_tokens_default ?? 1024),
          video_ref_tokens_per_sec: {
            '480p': Number(values.video_ref_tokens_480p ?? 2886),
            '768p': Number(values.video_ref_tokens_768p ?? 7459),
          },
          audio_ref_tokens_per_sec: Number(values.audio_ref_tokens_per_sec ?? 80),
        };
      }
      if (Array.isArray(values.supported_models) && values.supported_models.length > 0) {
        extConfig.supported_models = values.supported_models;
      }
      if (values.billing_rule !== 'glm_5_3') {
        if (values.extended_config?.web_search_rate !== undefined) {
          extConfig.web_search_rate = values.extended_config.web_search_rate;
        } else if (values.web_search_rate !== undefined) {
          extConfig.web_search_rate = values.web_search_rate;
        }
      }

      // 将时间段倍率配置序列化为 HH:mm 并写入 extConfig
      const invertRemainderField = form.getFieldValue(['extended_config', 'invert_remainder_multiplier']);
      const invertRemainderRaw = invertRemainderField == null || invertRemainderField === '' ? NaN : Number(invertRemainderField);
      if (enableTimeMultipliers && (!Number.isFinite(invertRemainderRaw) || invertRemainderRaw < 0)) {
        message.error('请填写未规划时段倍率');
        return;
      }
      if (enableTimeMultipliers) {
        const timeInputTz = form.getFieldValue(['extended_config', 'time_input_tz']) === 'admin_display'
          ? 'admin_display'
          : 'operation';
        const storedMultipliers = convertScheduleSlots(
          timeMultipliers,
          scheduleInputTzName(timeInputTz, operationTz, adminTz),
          operationTz,
        );
        const storedIntervals = [];
        for (let i = 0; i < storedMultipliers.length; i++) {
          const item: any = storedMultipliers[i];
          const days = normalizeWeekdays(item.days);
          if (isAllDayMultiplier(item)) continue;
          if (!item.start || !item.end || typeof item.start.format !== 'function' || typeof item.end.format !== 'function') {
            message.error(`第 ${i + 1} 个时段折算到站点运营时区后无效，请改用运营时区填写或拆开全天规则`);
            return;
          }
          const startMin = item.start.hour() * 60 + item.start.minute();
          const endMin = item.end.hour() * 60 + item.end.minute();
          if (startMin === endMin) {
            message.error(`第 ${i + 1} 个时段折算后起止时间相同，未写入，避免计费空窗`);
            return;
          }
          storedIntervals.push(...expandTimeMultiplierIntervals(startMin, endMin, days, i));
        }
        if (findOverlappingTimeMultipliers(storedIntervals)) {
          message.error('时段折算到站点运营时区后出现重叠，请改用运营时区填写或调整星期');
          return;
        }
        const formattedMultipliers = storedMultipliers.map((item: any) => {
          const days = normalizeWeekdays(item.days);
          if (isAllDayMultiplier(item)) {
            return {
              all_day: true,
              multiplier: item.multiplier ?? 1.0,
              days,
            };
          }
          return {
            start: item.start.format("HH:mm"),
            end: item.end.format("HH:mm"),
            multiplier: item.multiplier ?? 1.0,
            days,
          };
        });
        extConfig = {
          ...extConfig,
          enable_time_multipliers: true,
          invert_time_multipliers: true,
          invert_remainder_multiplier: invertRemainderRaw,
          time_input_tz: timeInputTz,
          time_multipliers: formattedMultipliers,
        };
      } else {
        extConfig = {
          ...extConfig,
          enable_time_multipliers: false,
          invert_time_multipliers: false,
          time_multipliers: [],
        };
        delete extConfig.invert_remainder_multiplier;
      }

      // 清除表单中不应提交的临时字段
      delete values.sd2_resolutions;
      delete values.volc_audio_rate; delete values.volc_base_rate; delete values.volc_offline_discount;
      delete values.s1_online_rate; delete values.s1_offline_rate;
      delete values.image_prompt_rate;
      delete values.prompt_extend_multiplier;
      delete values.image_ref_multiplier;
      delete values.kling_mode_std; delete values.kling_mode_pro; delete values.kling_mode_2k; delete values.kling_mode_4k;
      delete values.kling_sound_off; delete values.kling_sound_on;
      delete values.kling_video_ref_no; delete values.kling_video_ref_yes;
      delete values.supported_models;
      delete values.kling_enable_mode; delete values.kling_enable_sound; delete values.kling_enable_video_ref;
      delete values.kling_use_price_table;
      delete values.quality_pricing_enabled;
      delete values.doubao_fast_enabled;
      delete values.layer_pricing_enabled;
      delete values.vidu_offpeak_discount;
      delete values.free_image_count;
      delete values.enable_video_ref;
      if (values.billing_rule === 'glm_5_3') {
        values.enable_claude_cache_creation = false;
        values.enable_claude_cache_read = false;
      }
      // 开关关闭时清零对应费率，防止旧值残留
      if (!values.enable_cached_rate) values.cached_rate = 0;
      if (!values.enable_claude_cache_creation) values.claude_cache_creation_rate = 0;
      if (!values.enable_claude_cache_read) values.claude_cache_read_rate = 0;
      delete values.enable_cached_rate;
      delete values.enable_claude_cache_creation;
      delete values.enable_claude_cache_read;

      const payload = {
        prompt_rate: 0,
        completion_rate: 0,
        fixed_rate: 0,
        duration_rate: 0,
        ...values,
        provider_id: values.provider_id || null,
        type_id: values.type_id || null,
        cached_rate: values.cached_rate || 0,
        claude_cache_creation_rate: values.claude_cache_creation_rate || 0,
        claude_cache_read_rate: values.claude_cache_read_rate || 0,
        pricing_tiers: values.pricing_tiers?.map((tier: any) => {
          const t = {
            ...tier,
            cached_rate: tier.cached_rate || 0,
            cache_write_rate: tier.cache_write_rate || 0,
          };
          t.enabled ??= true;
          if (values.billing_rule === 'image_size_pixel') {
            t.quality_pricing = form.getFieldValue('quality_pricing_enabled') === true;
          }
          if (values.billing_rule === 'doubao_chat' && !form.getFieldValue('doubao_fast_enabled')) {
            t.fast_prompt_rate = 0; t.fast_completion_rate = 0; t.fast_cached_rate = 0;
            t.fast_audio_prompt_rate = 0; t.fast_audio_cached_rate = 0;
          }
          if (values.billing_rule === 'volc_seedream_pro' && !form.getFieldValue('layer_pricing_enabled')) {
            t.layer_rate = 0;
          }
          if (values.billing_rule === 'video_seconds_ref') {
            const isRefAware = form.getFieldValue('enable_video_ref') !== false;
            if (isRefAware) {
              t.rate_no_ref = Number(tier.rate_no_ref ?? tier.rate ?? 0);
              t.rate_ref = Number(tier.rate_ref ?? tier.rate ?? 0);
              t.input_rate_ref = Number(tier.input_rate_ref ?? tier.input_rate ?? 0);
              t.rate = t.rate_no_ref;
              t.input_rate = t.input_rate_ref;
            } else {
              t.rate = Number(tier.rate ?? 0);
              t.input_rate = Number(tier.input_rate ?? 0);
            }
          }
          return t;
        }) || [],
        extended_config: extConfig,
        is_active: 1,
      };
      // PID 由服务端按规则生成（手动 6xxxx），管理后台不提交、不改写
      delete payload.pid;

      const wasCreate = !editingItem;
      let saved: any;
      if (editingItem) {
        saved = await request.put(`/billing-rules/${editingItem.id}`, payload);
      } else {
        saved = await request.post('/billing-rules', payload);
      }
      message.success(t('common.success'));
      const rule = saved?.id ? saved : saved?.data;
      if (stayOnPage) {
        if (wasCreate && rule?.id) {
          hydrateBillingEditor(rule);
          hydratedEditIdRef.current = String(rule.pid || rule.id);
          navigate(billingRulesEditPath(adminPath, rule.pid || rule.id), { replace: true });
        }
      } else {
        handleCloseEditor();
      }
      fetchItems();
    } catch (e) {
      console.error(e);
    } finally {
      setSaving(false);
    }
  };

  const saveAndStay = async () => {
    try {
      const values = await form.validateFields();
      await handleSave(values, true);
    } catch {
      // 校验失败由表单项提示
    }
  };

  const handleRestoreDefault = async () => {
    if (!editingItem) return;
    Modal.confirm({
      title: '确认恢复默认？',
      content: '此操作将覆盖当前计费规则下的所有费率、定价阶梯与配置，恢复为系统默认初始状态。此操作不可撤销，是否继续？',
      okText: '确认',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          const resp = await (request.post(`/billing-rules/${editingItem.id}/restore-default`) as any);
          message.success('恢复默认成功');
          // 更新表单回显与编辑状态
          hydrateBillingEditor(resp);
          // 刷新列表
          fetchItems();
        } catch (e: any) {
          console.error(e);
        }
      }
    });
  };

  const columns = [
    {
      title: '计费 (PID)',
      dataIndex: 'pid',
      key: 'pid',
      width: 120,
      render: (text: string, record: BillingRuleData) => (
        <Tag 
          color="blue" 
          style={{ cursor: 'pointer' }}
          onClick={() => handleEdit(record)}
          title="点击编辑计费规则"
        >
          {text || '-'}
        </Tag>
      )
    },
    {
      title: '计费策略集命名',
      dataIndex: 'name',
      key: 'name',
      width: 200,
      render: (text: string, record: BillingRuleData) => (
        <Text 
          strong 
          style={{ cursor: 'pointer' }}
          onClick={() => handleEdit(record)}
          title="点击编辑计费规则"
        >
          {text}
        </Text>
      )
    },
    {
      title: '服务商 / 分类 / 计费',
      key: 'attributes',
      width: 250,
      render: (_: any, record: BillingRuleData) => {
        const p = allProviders.find(x => x.id === record.provider_id);
        const tObj = allTypes.find(x => x.id === record.type_id);
        const colors: Record<string, string> = { tokens: 'cyan', requests: 'orange', duration: 'purple' };

        return (
          <Space size={[4, 4]} wrap>
            {p && <Tag color="default" style={{ margin: 0 }}>{p.name}</Tag>}
            {tObj && <Tag color="processing" style={{ margin: 0 }}>{tObj.name}</Tag>}
            <Tag color={record.pricing_type === 'official' ? 'orange' : 'green'} style={{ margin: 0 }}>{record.pricing_type === 'official' ? '官方计费' : '自定义计费'}</Tag>
            <Tag color={colors[record.billing_type]} style={{ margin: 0 }}>{t(`models.type_${record.billing_type}`)}</Tag>
          </Space>
        );
      }
    },
    {
      title: t('models.rates'),
      key: 'rates',
      width: 320,
      render: (_: any, record: BillingRuleData) => {
        return <RateDisplay rule={record} currencySymbol={currencySymbol} formatPrice={formatPrice} />;
      }
    },
    {
      title: '规则来源',
      dataIndex: 'is_system',
      key: 'is_system',
      render: (isSystem: number) => {
        const isSys = isSystem === 1;
        return (
          <Tag color={isSys ? 'blue' : 'orange'}>
            {isSys ? '系统规则' : '手动添加'}
          </Tag>
        );
      },
      width: 120,
    },
    {
      title: '排序',
      dataIndex: 'sort_order',
      key: 'sort_order',
      width: 100,
      sorter: (a: BillingRuleData, b: BillingRuleData) => (a.sort_order || 0) - (b.sort_order || 0),
      render: (sort_order: number) => <Text type="secondary" style={{ fontSize: 13 }}>{sort_order || 0}</Text>,
    },
    {
      title: '最后修改时间',
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 170,
      sorter: (a: BillingRuleData, b: BillingRuleData) => {
        const timeA = a.updated_at ? dayjs(a.updated_at).valueOf() : 0;
        const timeB = b.updated_at ? dayjs(b.updated_at).valueOf() : 0;
        return timeA - timeB;
      },
      defaultSortOrder: 'descend' as const,
      render: (text: string) => <Text type="secondary" style={{ fontSize: 12 }}>{text ? dayjs(text).format('YYYY-MM-DD HH:mm:ss') : '-'}</Text>,
    },
    {
      title: t('common.actions'),
      key: 'actions',
      render: (_: any, record: BillingRuleData) => (
        <Space>
          <Button icon={<EditOutlined />} onClick={() => handleEdit(record)} size="small" />
          <Button icon={<CopyOutlined />} onClick={() => openCopy(record)} size="small" />
          {record.is_system === 1 ? (
            <Tooltip title="系统内置规则，不可删除">
              <Button icon={<DeleteOutlined />} disabled size="small" />
            </Tooltip>
          ) : (
            <Popconfirm title={t('common.confirm_delete')} onConfirm={() => handleDelete(record.id)}>
              <Button icon={<DeleteOutlined />} danger size="small" />
            </Popconfirm>
          )}
        </Space>
      ),
      width: 150,
    },
  ];

  const filteredItems = items.filter(item => {
    if (filterType !== 'all' && item.billing_type !== filterType) return false;
    if (filterPricingType === 'system' && item.is_system !== 1) return false;
    if (filterPricingType === 'manual' && item.is_system === 1) return false;
    if (searchText) {
      const lower = searchText.toLowerCase();
      if (!item.name?.toLowerCase().includes(lower) && !String(item.pid || '').toLowerCase().includes(lower)) {
        return false;
      }
    }
    if (filterProvider === 0 && item.provider_id) return false;
    if (filterProvider && item.provider_id !== filterProvider) return false;
    if (filterTypeSelect === 0 && item.type_id) return false;
    if (filterTypeSelect && item.type_id !== filterTypeSelect) return false;
    return true;
  });

  const paginationConfig = listPagination({
    current: currentPage,
    pageSize,
    onChange,
  });

  return (
    <>
      {!isEditorOpen && (
        <Card variant="borderless">
          <div style={{ display: 'flex', flexDirection: screens.xs ? 'column' : 'row', justifyContent: 'space-between', marginBottom: 16, gap: 12 }}>
            <Typography.Title level={4} style={{ margin: 0, fontSize: screens.xs ? 18 : 20, fontWeight: 600 }}>
              计费规则管理
            </Typography.Title>
            <Button type="primary" icon={<PlusOutlined />} onClick={handleAdd}>
              {screens.xs ? '新建' : '新建计费策略类'}
            </Button>
          </div>
          <div style={{ marginBottom: 20, display: 'flex', flexWrap: 'wrap', gap: 16, alignItems: 'center' }}>
            <Space wrap>
              <Text strong>筛选计费类型：</Text>
              <Radio.Group
                value={filterType}
                onChange={e => setFilterType(e.target.value)}
                optionType="button"
                buttonStyle="solid"
              >
                <Radio value="all">全部</Radio>
                <Radio value="tokens">{t('models.type_tokens')}</Radio>
                <Radio value="requests">{t('models.type_requests')}</Radio>
                <Radio value="duration">{t('models.type_duration')}</Radio>
              </Radio.Group>
            </Space>
            <Space wrap>
              <Text strong>规则来源：</Text>
              <Radio.Group
                value={filterPricingType}
                onChange={e => setFilterPricingType(e.target.value)}
                optionType="button"
                buttonStyle="solid"
              >
                <Radio value="all">全部</Radio>
                <Radio value="system">系统规则</Radio>
                <Radio value="manual">手动添加</Radio>
              </Radio.Group>
            </Space>
            <Input.Search
              placeholder="搜索名称或PID"
              allowClear
              value={searchText}
              onChange={e => setSearchText(e.target.value)}
              style={{ width: 200 }}
            />
            {auxiliaryCurrencies.length > 0 && (
              <Select
                value={selectedCurrencyCode}
                onChange={setSelectedCurrencyCode}
                style={{ width: 140 }}
                options={[
                  { value: '', label: `默认货币 (${currencySymbol})` },
                  ...auxiliaryCurrencies.map(c => ({ value: c.code, label: `${c.code} (${c.symbol})` }))
                ]}
              />
            )}
          </div>

          <ClassificationFilter
            providers={[
              ...allProviders.map(p => ({
                ...p,
                count: items.filter(i => i.provider_id != null && Number(i.provider_id) === Number(p.id)).length
              })),
              unclassifiedChip(items.filter(i => !i.provider_id).length),
            ]}
            types={[
              ...allTypes.map(t => ({
                ...t,
                count: items.filter(i => i.type_id != null && Number(i.type_id) === Number(t.id)).length
              })),
              unclassifiedChip(items.filter(i => !i.type_id).length),
            ]}
            selectedProvider={filterProvider}
            selectedType={filterTypeSelect}
            onProviderChange={setFilterProvider}
            onTypeChange={setFilterTypeSelect}
          />

          {screens.xs ? (
            <MobileCardList
              dataSource={filteredItems}
              loading={loading}
              rowKey="id"
              pagination={paginationConfig}
              renderCard={(record: any) => {
                const colors: Record<string, string> = { tokens: 'cyan', requests: 'orange', duration: 'purple' };
                return (
                  <MobileCard
                    title={<Text strong>{record.name}</Text>}
                    extra={
                      <Tag color={record.is_system === 1 ? 'blue' : 'orange'} style={{ margin: 0 }}>
                        {record.is_system === 1 ? '系统' : '手动'}
                      </Tag>
                    }
                  >
                    <CardRow label="计费 (PID)"><Tag color="blue">{record.pid || '-'}</Tag></CardRow>
                    <CardRow label="计费类型"><Tag color={colors[record.billing_type]}>{t(`models.type_${record.billing_type}`)}</Tag></CardRow>
                    <CardRow label="费率"><RateDisplay rule={record} currencySymbol={currencySymbol} formatPrice={formatPrice} /></CardRow>
                    <CardRow label="排序"><Text type="secondary" style={{ fontSize: 13 }}>{record.sort_order || 0}</Text></CardRow>
                    <CardRow label="最后修改"><Text type="secondary" style={{ fontSize: 12 }}>{record.updated_at ? dayjs(record.updated_at).format('YYYY-MM-DD HH:mm:ss') : '-'}</Text></CardRow>
                    <CardActions>
                      <Button size="small" icon={<EditOutlined />} onClick={() => handleEdit(record)} />
                      <Button size="small" icon={<CopyOutlined />} onClick={() => openCopy(record)} />
                      {record.is_system === 1 ? (
                        <Tooltip title="系统内置规则，不可删除">
                          <Button size="small" icon={<DeleteOutlined />} disabled />
                        </Tooltip>
                      ) : (
                        <Popconfirm title={t('common.confirm_delete')} onConfirm={() => handleDelete(record.id)}>
                          <Button size="small" icon={<DeleteOutlined />} danger />
                        </Popconfirm>
                      )}
                    </CardActions>
                  </MobileCard>
                );
              }}
            />
          ) : (
            <div ref={tableContainerRef}>
              <Table
                dataSource={filteredItems}
                columns={columns}
                rowKey="id"
                loading={loading}
                pagination={paginationConfig}
                size="middle"
                scroll={{ x: 'max-content' }}
              />
            </div>
          )}
        </Card>
      )}

      <Modal
        title="复制计费规则"
        open={!!copySource}
        onCancel={() => setCopySource(null)}
        onOk={commitCopy}
        confirmLoading={copying}
        okText="确定"
        cancelText="取消"
        destroyOnHidden
      >
        <div style={{ marginBottom: 8 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>
            将复制「{copySource?.name}」的全部计费参数，仅名称不同；新规则为手动添加，PID 自动生成
          </Text>
        </div>
        <Input
          autoFocus
          placeholder="请输入新规则名称"
          value={copyName}
          onChange={e => setCopyName(e.target.value)}
          onPressEnter={commitCopy}
        />
      </Modal>

      {isEditorOpen && (
        <Card
          title={editingItem ? '编辑计费基础组' : '生成新的计费规则'}
          extra={
            <div style={{ display: 'flex', gap: '8px' }}>
              <Button onClick={handleCloseEditor} disabled={saving}>取消</Button>
              <Button type="primary" loading={saving} onClick={saveAndStay}>保存更新</Button>
              {editingItem?.is_system === 1 && (
                <Button danger onClick={handleRestoreDefault} disabled={saving}>恢复默认</Button>
              )}
              <Button loading={saving} onClick={() => form.submit()}>保存退出</Button>
            </div>
          }
        >
          <div style={{ maxWidth: 1400, margin: '0 auto' }}>
            <Form form={form} layout="vertical" onFinish={(values) => handleSave(values, false)}>
              <Row gutter={16}>
                <Col xs={24} sm={12}>
                  <Form.Item name="name" label="大模型计费模版名称" rules={[{ required: true }]}>
                    <Input placeholder="输入该组计费的明显记号，方便添加模型时直接下拉应用。" />
                  </Form.Item>
                </Col>
                <Col xs={24} sm={6}>
                  <Form.Item
                    name="pid"
                    label="计费规则 PID"
                    tooltip={editingItem ? "PID 由系统按规则生成，唯一且不可修改" : "无需填写。保存后自动生成：手动规则为 6 开头的 5 位数字"}
                  >
                    <Input placeholder="保存后自动生成" disabled />
                  </Form.Item>
                </Col>
                <Col xs={24} sm={6}>
                  <Form.Item name="sort_order" label="排序" tooltip="数字越大越靠前，默认 0">
                    <InputNumber min={0} style={{ width: '100%' }} placeholder="排序" />
                  </Form.Item>
                </Col>
              </Row>

              <Form.Item name="supported_models" label={<Text type="secondary">支持模型ID (非必填，仅作提示)</Text>} tooltip="可输入多个模型 ID，按回车确认，用于提示哪些模型可以使用此计费规则">
                <Select mode="tags" open={false} placeholder="输入支持的模型 ID，按回车确认，例如: gpt-4, claude-3-opus" style={{ width: '100%' }} />
              </Form.Item>

              <Row gutter={24}>
                <Col span={12}>
                  <Form.Item name="provider_id" label={<Text type="secondary">所属服务商 (选填，仅作筛选用)</Text>} style={{ marginBottom: 16 }}>
                    <Radio.Group size="small" optionType="button" buttonStyle="solid" style={{ display: 'flex', flexWrap: 'wrap', gap: '6px 8px' }}>
                      <Radio value={null}>无 (不绑定)</Radio>
                      {allProviders.map(p => (
                        <Radio key={p.id} value={p.id}>{p.name}</Radio>
                      ))}
                    </Radio.Group>
                  </Form.Item>
                </Col>
                <Col span={12}>
                  <Form.Item name="type_id" label={<Text type="secondary">模型分类 (选填，仅作筛选用)</Text>} style={{ marginBottom: 16 }}>
                    <Radio.Group size="small" optionType="button" buttonStyle="solid" style={{ display: 'flex', flexWrap: 'wrap', gap: '6px 8px' }}>
                      <Radio value={null}>无 (不绑定)</Radio>
                      {allTypes.map(t => (
                        <Radio key={t.id} value={t.id}>{t.name}</Radio>
                      ))}
                    </Radio.Group>
                  </Form.Item>
                </Col>
              </Row>

              <Row gutter={24}>
                <Col span={12}>
                  <Form.Item name="pricing_type" label={<Text type="secondary">定价类型</Text>} style={{ marginBottom: 16 }}>
                    <Radio.Group size="small" optionType="button" buttonStyle="solid" style={{ display: 'flex', gap: 8 }}>
                      <Radio value="custom">自定义计费</Radio>
                      <Radio value="official">官方计费</Radio>
                    </Radio.Group>
                  </Form.Item>
                </Col>
                <Col span={12}>
                  <Form.Item name="billing_type" label={<Text type="secondary">{t('models.billing_type')}</Text>} rules={[{ required: true }]} style={{ marginBottom: 16 }}>
                    <Radio.Group size="small" optionType="button" buttonStyle="solid" style={{ display: 'flex', gap: 8 }} onChange={(e) => {
                      const type = e.target.value;
                      setBillingType(type);
                      if (type === 'tokens') form.setFieldsValue({ billing_rule: 'standard' });
                      else if (type === 'requests') form.setFieldsValue({ billing_rule: 'fixed' });
                      else if (type === 'duration') form.setFieldsValue({ billing_rule: 'standard' });
                    }}>
                      <Radio value="tokens">{t('models.type_tokens')}</Radio>
                      <Radio value="requests">{t('models.type_requests')}</Radio>
                      <Radio value="duration">{t('models.type_duration')}</Radio>
                    </Radio.Group>
                  </Form.Item>
                </Col>
              </Row>

              {billingType === 'tokens' && (
                <>
                  <Form.Item name="billing_rule" label={t('models.billing_rule')} initialValue="standard" rules={[{ required: true, message: '请选择计费规则' }]}>
                    <Radio.Group optionType="button" buttonStyle="solid">
                      <Radio value="standard">{t('models.rule_standard')}</Radio>
                      <Radio value="multimodal">{t('models.rule_multimodal')}</Radio>
                      <Radio value="gpt_billing">GPT图片计费</Radio>
                      <Radio value="tiered">{t('models.rule_tiered')}</Radio>
                      <Radio value="doubao_chat">豆包聊天阶梯</Radio>
                      <Radio value="seedance2.0">Seedance 2 规则</Radio>
                      <Radio value="seedance1.5pro">Seedance 1.5 规则</Radio>
                      <Radio value="seedance1.0">Seedance 1 规则</Radio>
                      <Radio value="glm_5_3">glm 5.3</Radio>
                    </Radio.Group>
                  </Form.Item>

                  <Form.Item noStyle shouldUpdate={(prev, curr) => prev.billing_rule !== curr.billing_rule}>
                    {({ getFieldValue }) => {
                      const rule = getFieldValue('billing_rule');
                      const unitLabel = t('models.prompt_rate');
                      const unitLabelComp = t('models.completion_rate');

                      if (rule === 'gpt_billing') {
                        const items = [
                          { key: 'input_text', label: '输入文本 (Text Input)' },
                          { key: 'input_image', label: '输入图片 (Image Input)' },
                          { key: 'output_image', label: '输出图片 (Image Output)' },
                          { key: 'cached_input_text', label: '输入文本缓存 (Text Cached Input)' },
                          { key: 'cached_input_image', label: '输入图片缓存 (Image Cached Input)' },
                        ];

                        return (
                          <RuleContainer isLight={_isLight} title="GPT图片计费配置" description="面向图片/生图模型：文本输入、图片输入、图片输出及缓存可分别开关，按每 1M tokens 计价。仅出图时用「输出图片」项即可，无需配置输出文本。">
                            <Row gutter={16}>
                              {items.map(item => (
                                <Col span={12} key={item.key}>
                                  <Form.Item noStyle dependencies={[`gpt_${item.key}_enabled`]}>
                                    {({ getFieldValue: gfv }) => {
                                      const enabled = gfv(`gpt_${item.key}_enabled`);
                                      return (
                                        <div style={{
                                          padding: '12px 16px',
                                          background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)',
                                          borderRadius: 8,
                                          marginBottom: 12,
                                          border: _isLight ? '1px solid #f0f0f0' : '1px solid #303030'
                                        }}>
                                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: enabled ? 12 : 0 }}>
                                            <span style={{ fontSize: 13, fontWeight: 500 }}>{item.label}</span>
                                            <Form.Item name={`gpt_${item.key}_enabled`} valuePropName="checked" style={{ margin: 0 }}>
                                              <Switch size="small" />
                                            </Form.Item>
                                          </div>
                                          {enabled && (
                                            <Form.Item name={`gpt_${item.key}_rate`} style={{ margin: 0 }} rules={[{ required: true, message: '请输入价格' }]}>
                                              <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" placeholder="输入价格" />
                                            </Form.Item>
                                          )}
                                        </div>
                                      );
                                    }}
                                  </Form.Item>
                                </Col>
                              ))}
                            </Row>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'multimodal') {
                        return (
                          <RuleContainer isLight={_isLight} title={t('models.rule_multimodal')}>
                            <Row gutter={16}>
                              <Col span={12}>
                                <Form.Item name="prompt_rate" label={t('models.text_input_rate')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item name="image_prompt_rate" label={t('models.image_input_rate')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                            </Row>
                            <Row gutter={16} style={{ marginTop: 16 }}>
                              <Col span={12}>
                                <Form.Item name="completion_rate" label={t('models.text_completion_rate', '文本/思考输出单价')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item name="image_completion_rate" label={t('models.image_completion_rate', '图片输出单价')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                            </Row>
                            <WebSearchRateControl isLight={_isLight} currencyUnit={currencyUnit} />
                          </RuleContainer>
                        );
                      }

                      if (rule === 'standard' || rule === 'glm_5_3') {
                        const isGlm53 = rule === 'glm_5_3';
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title={isGlm53 ? 'glm 5.3' : t('models.rule_standard')}
                            description={isGlm53 ? '按输入（含缓存）/输出/缓存命中拆账；差异 usage 输入=命中+未命中。' : undefined}
                          >
                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={12}>
                                <Form.Item name="prompt_rate" label={unitLabel} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item name="completion_rate" label={unitLabelComp} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" />
                                </Form.Item>
                              </Col>
                            </Row>
                            <CacheRateControl name="enable_cached_rate" rateName="cached_rate" label={isGlm53 ? '缓存命中费率' : '缓存命中费率（通用缓存读取）'} isLight={_isLight} />
                            {!isGlm53 && (
                              <>
                                <CacheRateControl name="enable_claude_cache_creation" rateName="claude_cache_creation_rate" label="Claude 缓存创建费率" isLight={_isLight} />
                                <CacheRateControl name="enable_claude_cache_read" rateName="claude_cache_read_rate" label="Claude 缓存读取费率" isLight={_isLight} />
                                <WebSearchRateControl isLight={_isLight} currencyUnit={currencyUnit} />
                              </>
                            )}
                          </RuleContainer>
                        );
                      }

                      if (rule === 'seedance2.0') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="Seedance 2.0 计费配置"
                            description="指定具体支持的视频分辨率及是否包含视频输入的定价 (可分级管控)。匹配说明：按分辨率参数精确匹配（如 480p, 720p, 1080p, 4k）。若未命中且未配置，默认兜底使用 720p 档位。"
                          >
                            <Form.List name="sd2_resolutions" initialValue={[
                              { resolution: '480p', enabled: false, with_video: 0, without_video: 0 },
                              { resolution: '720p', enabled: false, with_video: 0, without_video: 0 },
                              { resolution: '1080p', enabled: false, with_video: 0, without_video: 0 },
                              { resolution: '4k', enabled: false, with_video: 0, without_video: 0 }
                            ]}>
                              {(fields, { add, remove }) => (
                                <>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <div key={key} style={{ marginBottom: 16, padding: '12px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: '8px' }}>
                                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                          <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                            <Input placeholder="分辨率 (如 4k)" style={{ width: 120 }} size="small" />
                                          </Form.Item>
                                          <Text strong style={{ fontSize: '13px' }}>分辨率矩阵计费</Text>
                                        </div>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                          <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ margin: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                          <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} size="small" />
                                        </div>
                                      </div>
                                      <Form.Item noStyle dependencies={[['sd2_resolutions', name, 'enabled']]}>
                                        {({ getFieldValue }) => {
                                          const isEnabled = getFieldValue(['sd2_resolutions', name, 'enabled']);
                                          const resValue = getFieldValue(['sd2_resolutions', name, 'resolution']) || '此分辨率';
                                          return isEnabled ? (
                                            <Row gutter={16} align="middle">
                                              <Col span={12}>
                                                <Form.Item {...restField} name={[name, 'with_video']} label={<Text style={{ fontSize: '12px' }}>包含视频输入 ({currencyUnit}/百万)</Text>} rules={[{ required: true, message: '' }]} style={{ marginBottom: 0 }}>
                                                  <InputNumber style={{ width: '100%' }} precision={6} min={0} />
                                                </Form.Item>
                                              </Col>
                                              <Col span={12}>
                                                <Form.Item {...restField} name={[name, 'without_video']} label={<Text style={{ fontSize: '12px' }}>不包含视频输入 ({currencyUnit}/百万)</Text>} rules={[{ required: true, message: '' }]} style={{ marginBottom: 0 }}>
                                                  <InputNumber style={{ width: '100%' }} precision={6} min={0} />
                                                </Form.Item>
                                              </Col>
                                            </Row>
                                          ) : (
                                            <Text type="secondary" style={{ fontSize: '12px' }}>{resValue} 被关闭，将不响应此分辨率独立的按需定价。</Text>
                                          );
                                        }}
                                      </Form.Item>
                                    </div>
                                  ))}
                                  <Button
                                    type="dashed"
                                    onClick={() => add({ resolution: '', enabled: true, with_video: 0, without_video: 0 })}
                                    block
                                    icon={<PlusOutlined />}
                                    style={{ marginTop: 8, height: '40px' }}
                                  >
                                    添加一条 Seedance 2.0 分辨率计费阶梯
                                  </Button>
                                </>
                              )}
                            </Form.List>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'seedance1.5pro') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            description="如需支持离线推理(flex)降价，请在此配置乘以的折扣倍率"
                          >
                            <Row gutter={16} align="middle">
                              <Col span={8}><Form.Item name="volc_audio_rate" label="包含语音" rules={[{ required: true }]} style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" /></Form.Item></Col>
                              <Col span={8}><Form.Item name="volc_base_rate" label="不包含语音" rules={[{ required: true }]} style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" /></Form.Item></Col>
                              <Col span={8}><Form.Item name="volc_offline_discount" label="离线推理(flex)折扣倍率" tooltip="例如 0.5 即等于最终价格减半" rules={[{ required: true }]} style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} /></Form.Item></Col>
                            </Row>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'seedance1.0') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            description="支持在线与离线的双轨计费"
                          >
                            <Row gutter={16} align="middle">
                              <Col span={12}><Form.Item name="s1_online_rate" label="在线推理定价" rules={[{ required: true }]} style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" /></Form.Item></Col>
                              <Col span={12}><Form.Item name="s1_offline_rate" label="离线推理(flex)定价" rules={[{ required: true }]} style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ 1M" /></Form.Item></Col>
                            </Row>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'tiered') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title={t('models.pricing_tiers')}
                            description="上限单位千(K)，如 272=≤272K。读/写缓存选填，空则按输入价。费用=(未缓存入×入价 + 读缓存×读价 + 写缓存×写价 + 出×出价)/1M。"
                          >
                            <Form.List name="pricing_tiers" initialValue={[]}>
                              {(fields, { add, remove }) => (
                                <>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <Row key={key} gutter={[8, 8]} align="middle" style={{ marginBottom: 12 }}>
                                      <Col span={6}>
                                        <Space.Compact style={{ width: '100%' }}>
                                          <Form.Item {...restField} name={[name, 'max_prompt_tokens']} rules={[{ required: true, message: '' }]} noStyle>
                                            <InputNumber placeholder="输入上限(K)" style={{ width: '50%' }} />
                                          </Form.Item>
                                          <Form.Item {...restField} name={[name, 'max_completion_tokens']} noStyle>
                                            <InputNumber placeholder="输出上限(K)" style={{ width: '50%' }} />
                                          </Form.Item>
                                        </Space.Compact>
                                      </Col>
                                      <Col span={4}>
                                        <Form.Item {...restField} name={[name, 'prompt_rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder={t('models.input_rate')} style={{ width: '100%' }} precision={6} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={4}>
                                        <Form.Item {...restField} name={[name, 'completion_rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder={t('models.output_rate')} style={{ width: '100%' }} precision={6} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={4}>
                                        <Form.Item {...restField} name={[name, 'cached_rate']} noStyle>
                                          <InputNumber placeholder="读缓存" style={{ width: '100%' }} precision={6} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={4}>
                                        <Form.Item {...restField} name={[name, 'cache_write_rate']} noStyle>
                                          <InputNumber placeholder="写缓存" style={{ width: '100%' }} precision={6} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={2} style={{ textAlign: 'right' }}>
                                        <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} />
                                      </Col>
                                    </Row>
                                  ))}
                                  <Button
                                    type="dashed"
                                    onClick={() => add()}
                                    block
                                    icon={<PlusOutlined />}
                                    style={{ marginTop: 8, height: '40px' }}
                                  >
                                    添加阶梯
                                  </Button>
                                </>
                              )}
                            </Form.List>
                            <WebSearchRateControl isLight={_isLight} currencyUnit={currencyUnit} />
                          </RuleContainer>
                        );
                      }

                      if (rule === 'doubao_chat') {
                        return (
                          <Form.Item noStyle dependencies={['doubao_fast_enabled']}>
                            {({ getFieldValue: gfv }) => {
                              const fastEnabled = gfv('doubao_fast_enabled');
                              return (
                                <RuleContainer
                                  isLight={_isLight}
                                  title="豆包聊天阶梯计费"
                                  extra={
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                      <Text style={{ fontSize: 12 }}>低延迟</Text>
                                      <Form.Item name="doubao_fast_enabled" valuePropName="checked" style={{ margin: 0 }}>
                                        <Switch size="small" />
                                      </Form.Item>
                                    </div>
                                  }
                                  description={`上限单位千(K)，入/出同时命中（出不填=不限）。费用=(非音频入×价 + 音频入×价 + 非音频缓存×价 + 音频缓存×价 + 出×价)/1M。${fastEnabled ? '低延迟开启后：service_tier=fast 用低延迟费率，未填则回退常规。' : ''}`}
                                >
                                  <Form.List name="pricing_tiers" initialValue={[]}>
                                    {(fields, { add, remove }) => (
                                      <>
                                        {/* 表头 */}
                                        <Row gutter={8} style={{ marginBottom: 6, opacity: 0.6, fontSize: 11, paddingLeft: fastEnabled ? '45px' : 0 }}>
                                          <Col span={4}>上下文区间</Col>
                                          <Col span={4}>输入(非音频)</Col>
                                          <Col span={3}>输出</Col>
                                          <Col span={4}>缓存(非音频)</Col>
                                          <Col span={4}>输入(音频)</Col>
                                          <Col span={4}>缓存(音频)</Col>
                                          <Col span={1}></Col>
                                        </Row>
                                        {fields.map(({ key, name, ...restField }) => (
                                          <Row key={key} gutter={8} align="middle" style={{
                                            position: 'relative',
                                            paddingLeft: fastEnabled ? '45px' : 0,
                                            marginBottom: fastEnabled ? 16 : 10,
                                            paddingTop: fastEnabled ? '10px' : '4px',
                                            paddingBottom: fastEnabled ? '10px' : '4px',
                                            borderBottom: fastEnabled ? (_isLight ? '1px dashed #e8e8e8' : '1px dashed #303030') : 'none'
                                          }}>
                                            {fastEnabled && (
                                              <div style={{ position: 'absolute', left: 0, top: '10px', display: 'flex', flexDirection: 'column', gap: '8px', flexShrink: 0 }}>
                                                <div style={{ height: '32px', display: 'flex', alignItems: 'center' }}>
                                                  <Tag color="success" style={{ margin: 0, fontSize: '10px', padding: '0 4px', lineHeight: '18px' }}>常规</Tag>
                                                </div>
                                                <div style={{ height: '32px', display: 'flex', alignItems: 'center' }}>
                                                  <Tag color="warning" style={{ margin: 0, fontSize: '10px', padding: '0 4px', lineHeight: '18px' }}>低延迟</Tag>
                                                </div>
                                              </div>
                                            )}
                                            {/* 第一列：上下文区间（两个输入框） */}
                                            <Col span={4}>
                                              <Space.Compact style={{ width: '100%' }}>
                                                <Form.Item {...restField} name={[name, 'max_prompt_tokens']} rules={[{ required: true, message: '' }]} noStyle>
                                                  <InputNumber placeholder="输入(K)" style={{ width: '50%' }} />
                                                </Form.Item>
                                                <Form.Item {...restField} name={[name, 'max_completion_tokens']} noStyle>
                                                  <InputNumber placeholder="输出(K)" style={{ width: '50%' }} />
                                                </Form.Item>
                                              </Space.Compact>
                                            </Col>
                                            {/* 第二列：输入(非音频)费率 */}
                                            <Col span={4}>
                                              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                                <Form.Item {...restField} name={[name, 'prompt_rate']} rules={[{ required: true }]} noStyle>
                                                  <InputNumber placeholder="常规·非音频" style={{ width: '100%' }} precision={6} />
                                                </Form.Item>
                                                {fastEnabled && (
                                                  <Form.Item {...restField} name={[name, 'fast_prompt_rate']} noStyle>
                                                    <InputNumber placeholder="低延迟·非音频" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} />
                                                  </Form.Item>
                                                )}
                                              </div>
                                            </Col>
                                            {/* 第三列：输出费率 */}
                                            <Col span={3}>
                                              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                                <Form.Item {...restField} name={[name, 'completion_rate']} rules={[{ required: true }]} noStyle>
                                                  <InputNumber placeholder="常规·输出" style={{ width: '100%' }} precision={6} />
                                                </Form.Item>
                                                {fastEnabled && (
                                                  <Form.Item {...restField} name={[name, 'fast_completion_rate']} noStyle>
                                                    <InputNumber placeholder="低延迟·输出" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} />
                                                  </Form.Item>
                                                )}
                                              </div>
                                            </Col>
                                            {/* 第四列：缓存(非音频)费率 */}
                                            <Col span={4}>
                                              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                                <Form.Item {...restField} name={[name, 'cached_rate']} noStyle>
                                                  <InputNumber placeholder="常规·缓存" style={{ width: '100%' }} precision={6} />
                                                </Form.Item>
                                                {fastEnabled && (
                                                  <Form.Item {...restField} name={[name, 'fast_cached_rate']} noStyle>
                                                    <InputNumber placeholder="低延迟·缓存" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} />
                                                  </Form.Item>
                                                )}
                                              </div>
                                            </Col>
                                            {/* 第五列：输入(音频)费率 */}
                                            <Col span={4}>
                                              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                                <Form.Item {...restField} name={[name, 'audio_prompt_rate']} noStyle>
                                                  <InputNumber placeholder="常规·音频" style={{ width: '100%' }} precision={6} />
                                                </Form.Item>
                                                {fastEnabled && (
                                                  <Form.Item {...restField} name={[name, 'fast_audio_prompt_rate']} noStyle>
                                                    <InputNumber placeholder="低延迟·音频" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} />
                                                  </Form.Item>
                                                )}
                                              </div>
                                            </Col>
                                            {/* 第六列：缓存(音频)费率 */}
                                            <Col span={4}>
                                              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                                                <Form.Item {...restField} name={[name, 'audio_cached_rate']} noStyle>
                                                  <InputNumber placeholder="常规·音频缓存" style={{ width: '100%' }} precision={6} />
                                                </Form.Item>
                                                {fastEnabled && (
                                                  <Form.Item {...restField} name={[name, 'fast_audio_cached_rate']} noStyle>
                                                    <InputNumber placeholder="低延迟·音频缓存" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} />
                                                  </Form.Item>
                                                )}
                                              </div>
                                            </Col>
                                            {/* 第七列：操作区（删除按钮） */}
                                            <Col span={1} style={{ textAlign: 'right' }}>
                                              <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} size="small" />
                                            </Col>
                                          </Row>
                                        ))}
                                        <Button
                                          type="dashed"
                                          onClick={() => add()}
                                          block
                                          icon={<PlusOutlined />}
                                          style={{ marginTop: 8, height: '40px' }}
                                        >
                                          添加一条豆包聊天阶梯
                                        </Button>
                                      </>
                                    )}
                                  </Form.List>
                                  <WebSearchRateControl isLight={_isLight} currencyUnit={currencyUnit} />
                                </RuleContainer>
                              );
                            }}
                          </Form.Item>
                        );
                      }

                      return null;
                    }}
                  </Form.Item>
                </>
              )}


              {billingType === 'requests' && (
                <>
                  <Form.Item name="billing_rule" label="计费子模式配置" initialValue="fixed" rules={[{ required: true, message: '请选择计费子模式' }]}>
                    <Radio.Group optionType="button" buttonStyle="solid">
                      <Radio value="fixed">固定费率</Radio>
                      <Radio value="per_image">按张收费</Radio>
                      <Radio value="image_resolution">分辨率 K</Radio>
                      <Radio value="image_resolution_io">分辨率 K(双向)</Radio>
                      <Radio value="image_size_pixel">分辨率像素</Radio>
                      <Radio value="vidu_image">Vidu 图片</Radio>
                      <Radio value="volc_seedream_pro">火山 Seedream 5.0 Pro</Radio>
                      <Radio value="characters">字符计费</Radio>
                    </Radio.Group>
                  </Form.Item>

                  <Form.Item noStyle shouldUpdate={(prev, curr) => prev.billing_rule !== curr.billing_rule}>
                    {({ getFieldValue }) => {
                      const rule = getFieldValue('billing_rule');

                      if (rule === 'image_resolution') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="图片分辨率K计费配置"
                            description="按分辨率等级 (1k/2k/4k) 计费的生图模型，按 resolution 参数匹配单张费率，支持图生图倍率。"
                          >
                            <Form.List name="pricing_tiers" initialValue={[]}>
                              {(fields, { add, remove }) => (
                                <>
                                  <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                    <Col span={7}>分辨率</Col>
                                    <Col span={6}>单张费率</Col>
                                    <Col span={5}>有图倍率 (图生图)</Col>
                                    <Col span={4}>状态</Col>
                                  </Row>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                          <Input placeholder="分辨率等级 (如: 1k)" style={{ width: '100%' }} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={6}>
                                        <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder="单张费率" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={5}>
                                        <Form.Item {...restField} name={[name, 'image_ref_multiplier']} noStyle>
                                          <InputNumber placeholder="参考图倍率" style={{ width: '100%' }} precision={2} step={0.1} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={4}>
                                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                          <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                          <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                        </div>
                                      </Col>
                                    </Row>
                                  ))}
                                  <Button
                                    type="dashed"
                                    onClick={() => add({ resolution: '', rate: 0, image_ref_multiplier: 1.0, enabled: true })}
                                    block
                                    icon={<PlusOutlined />}
                                    style={{ marginTop: 8, height: '40px' }}
                                  >
                                    增加一个分辨率价格档位
                                  </Button>
                                </>
                              )}
                            </Form.List>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'image_resolution_io') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="图片分辨率K（输入/生成）"
                            description="按 1k/2k/4k 分别设置输入与生成单张费率，适用于输入与生成分辨率费率分离的生图模型。"
                          >
                            <Form.List name="pricing_tiers" initialValue={[]}>
                              {(fields, { add, remove }) => (
                                <>
                                  <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                    <Col span={5}>分辨率</Col>
                                    <Col span={7}>输入 / 张</Col>
                                    <Col span={7}>生成 / 张</Col>
                                    <Col span={5}>状态</Col>
                                  </Row>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                      <Col span={5}>
                                        <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                          <Input placeholder="如: 1k" style={{ width: '100%' }} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'input_rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder="输入" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder="生成" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={5}>
                                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                          <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                          <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                        </div>
                                      </Col>
                                    </Row>
                                  ))}
                                  <Button
                                    type="dashed"
                                    onClick={() => add({ resolution: '', input_rate: 0, rate: 0, enabled: true })}
                                    block
                                    icon={<PlusOutlined />}
                                    style={{ marginTop: 8, height: '40px' }}
                                  >
                                    增加一个分辨率价格档位
                                  </Button>
                                </>
                              )}
                            </Form.List>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'image_size_pixel') {
                        return (
                          <Form.Item noStyle dependencies={['quality_pricing_enabled']}>
                            {({ getFieldValue: gfv }) => {
                              const qpEnabled = gfv('quality_pricing_enabled') === true;
                              return (
                                <RuleContainer
                                  isLight={_isLight}
                                  title="图片分辨率像素计费配置"
                                  extra={
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                      <Text style={{ fontSize: 12 }}>画质</Text>
                                      <Form.Item name="quality_pricing_enabled" valuePropName="checked" style={{ margin: 0 }}>
                                        <Switch size="small" />
                                      </Form.Item>
                                    </div>
                                  }
                                  description={`按像素宽高 (如 1024x1024) 计费的生图模型。${qpEnabled ? '已开启画质区分，按 low/medium/high 匹配不同画质费率。' : '按 size 参数匹配单张费率，支持图生图倍率。'}`}
                                >
                                  <Form.List name="pricing_tiers" initialValue={[]}>
                                    {(fields, { add, remove }) => (
                                      <>
                                        <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                          <Col span={qpEnabled ? 5 : 7}>像素分辨率</Col>
                                          {qpEnabled ? (
                                            <>
                                              <Col span={4}>低画质</Col>
                                              <Col span={4}>中画质</Col>
                                              <Col span={4}>高画质</Col>
                                            </>
                                          ) : (
                                            <Col span={6}>单张费率</Col>
                                          )}
                                          <Col span={qpEnabled ? 3 : 5}>有图倍率 (图生图)</Col>
                                          <Col span={3}>状态</Col>
                                        </Row>
                                        {fields.map(({ key, name, ...restField }) => (
                                          <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                            <Col span={qpEnabled ? 5 : 7}>
                                              <Form.Item {...restField} name={[name, 'size']} rules={[{ required: true }]} noStyle>
                                                <Input placeholder="如: 1024x1024" style={{ width: '100%' }} />
                                              </Form.Item>
                                            </Col>
                                            {qpEnabled ? (
                                              <>
                                                <Col span={4}>
                                                  <Form.Item {...restField} name={[name, 'rate_low']} rules={[{ required: true, message: '' }]} noStyle>
                                                    <InputNumber placeholder="低画质" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                                  </Form.Item>
                                                </Col>
                                                <Col span={4}>
                                                  <Form.Item {...restField} name={[name, 'rate_medium']} rules={[{ required: true, message: '' }]} noStyle>
                                                    <InputNumber placeholder="中画质" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                                  </Form.Item>
                                                </Col>
                                                <Col span={4}>
                                                  <Form.Item {...restField} name={[name, 'rate_high']} rules={[{ required: true, message: '' }]} noStyle>
                                                    <InputNumber placeholder="高画质" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                                  </Form.Item>
                                                </Col>
                                              </>
                                            ) : (
                                              <Col span={6}>
                                                <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                                  <InputNumber placeholder="单张费率" style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                                </Form.Item>
                                              </Col>
                                            )}
                                            <Col span={qpEnabled ? 3 : 5}>
                                              <Form.Item {...restField} name={[name, 'image_ref_multiplier']} noStyle>
                                                <InputNumber placeholder="有图倍率" style={{ width: '100%' }} precision={2} step={0.1} min={0} addonAfter="x" />
                                              </Form.Item>
                                            </Col>
                                            <Col span={3}>
                                              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                                <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                                  <Switch size="small" />
                                                </Form.Item>
                                                <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                              </div>
                                            </Col>
                                          </Row>
                                        ))}
                                        <Button
                                          type="dashed"
                                          onClick={() => add(qpEnabled
                                            ? { size: '', rate: 0, rate_low: 0, rate_medium: 0, rate_high: 0, quality_pricing: true, enabled: true, image_ref_multiplier: 1.0 }
                                            : { size: '', rate: 0, enabled: true, image_ref_multiplier: 1.0 }
                                          )}
                                          block
                                          icon={<PlusOutlined />}
                                          style={{ marginTop: 8, height: '40px' }}
                                        >
                                          增加一个像素分辨率价格档位
                                        </Button>
                                      </>
                                    )}
                                  </Form.List>
                                  <div style={{ marginTop: 16, padding: '12px 16px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8 }}>
                                    <Form.Item name="prompt_extend_multiplier" label="提示词扩写倍率" tooltip="当请求开启 prompt_extend 时，分辨率阶梯单价将乘以该倍率 (默认 1.0)" style={{ marginBottom: 0 }}>
                                      <InputNumber style={{ width: '100%', maxWidth: 280 }} precision={2} step={0.1} min={0} addonAfter="x" />
                                    </Form.Item>
                                  </div>
                                </RuleContainer>
                              );
                            }}
                          </Form.Item>
                        );
                      }

                      if (rule === 'volc_seedream_pro') {
                        return (
                          <Form.Item noStyle dependencies={['layer_pricing_enabled']}>
                            {({ getFieldValue: gfv }) => {
                              const layerEnabled = gfv('layer_pricing_enabled') === true;
                              return (
                                <RuleContainer
                                  isLight={_isLight}
                                  title="火山 Seedream 5.0 Pro 计费配置"
                                  extra={
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                      <Text style={{ fontSize: 12 }}>图层拆分</Text>
                                      <Form.Item name="layer_pricing_enabled" valuePropName="checked" style={{ margin: 0 }}>
                                        <Switch size="small" />
                                      </Form.Item>
                                    </div>
                                  }
                                  description={`输入超免费张数按张计，输出按像素档×张数；1K/1.5K/2K 按边长换算。官方 ≤261万 单图 0.30${layerEnabled ? ' / 图层 0.15' : ''}，以上 0.60${layerEnabled ? ' / 0.30' : ''}。${layerEnabled ? 'layer_decomposition=true 用图层价（空则回退单图）。' : ''}输入新建默认免 1 张。`}
                                >
                                  <Row gutter={16} style={{ marginBottom: 16 }}>
                                    <Col span={12}>
                                      <Form.Item name="prompt_rate" label="输入图额外单价" rules={[{ required: true, message: '请输入输入图额外单价' }]} style={{ marginBottom: 0 }}>
                                        <InputNumber placeholder="超出免费张数后每张价格" style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/张`} />
                                      </Form.Item>
                                    </Col>
                                    <Col span={12}>
                                      <Form.Item
                                        name="free_image_count"
                                        label="输入图免费张数"
                                        tooltip="不超过该数量的输入参考图不计费；新建默认 1。未配置的旧规则结算仍按首张免费。"
                                        initialValue={1}
                                        rules={[{ required: true, message: '请输入免费张数' }]}
                                        style={{ marginBottom: 0 }}
                                      >
                                        <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="张" />
                                      </Form.Item>
                                    </Col>
                                  </Row>

                                  <Form.Item label="输出图像素阶梯单价" required style={{ marginBottom: 0 }}>
                                    <Form.List name="pricing_tiers" initialValue={[]}>
                                      {(fields, { add, remove }) => (
                                        <>
                                          <Row gutter={8} style={{ marginBottom: 6, opacity: 0.6, fontSize: 11, paddingLeft: layerEnabled ? 45 : 0 }}>
                                            <Col span={layerEnabled ? 8 : 10}>总像素上限 (万像素)</Col>
                                            <Col span={layerEnabled ? 12 : 10}>{layerEnabled ? `单价 (${currencyUnit}/张)` : `单图费率 (${currencyUnit}/张)`}</Col>
                                            <Col span={4}>状态</Col>
                                          </Row>
                                          {fields.map(({ key, name, ...restField }) => (
                                            <Row key={key} gutter={8} align="middle" style={{
                                              position: 'relative',
                                              paddingLeft: layerEnabled ? 45 : 0,
                                              marginBottom: layerEnabled ? 16 : 12,
                                              paddingTop: layerEnabled ? 10 : 0,
                                              paddingBottom: layerEnabled ? 10 : 0,
                                              borderBottom: layerEnabled ? (_isLight ? '1px dashed #e8e8e8' : '1px dashed #303030') : 'none',
                                            }}>
                                              {layerEnabled && (
                                                <div style={{ position: 'absolute', left: 0, top: 10, display: 'flex', flexDirection: 'column', gap: 8 }}>
                                                  <div style={{ height: 32, display: 'flex', alignItems: 'center' }}>
                                                    <Tag color="success" style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '18px' }}>单图</Tag>
                                                  </div>
                                                  <div style={{ height: 32, display: 'flex', alignItems: 'center' }}>
                                                    <Tag color="warning" style={{ margin: 0, fontSize: 10, padding: '0 4px', lineHeight: '18px' }}>图层</Tag>
                                                  </div>
                                                </div>
                                              )}
                                              <Col span={layerEnabled ? 8 : 10}>
                                                <Form.Item {...restField} name={[name, 'max_pixels_wan']} rules={[{ required: true, message: '像素上限' }]} noStyle>
                                                  <InputNumber placeholder="例如: 261" style={{ width: '100%' }} precision={2} />
                                                </Form.Item>
                                              </Col>
                                              <Col span={layerEnabled ? 12 : 10}>
                                                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                                                  <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true, message: '单图费率' }]} noStyle>
                                                    <InputNumber placeholder={layerEnabled ? '单图' : '例如: 0.30'} style={{ width: '100%' }} precision={6} addonAfter="/张" />
                                                  </Form.Item>
                                                  {layerEnabled && (
                                                    <Form.Item {...restField} name={[name, 'layer_rate']} noStyle>
                                                      <InputNumber placeholder="图层（空则回退单图）" style={{ width: '100%', borderColor: _isLight ? '#ffe7ba' : '#874d00' }} precision={6} addonAfter="/张" />
                                                    </Form.Item>
                                                  )}
                                                </div>
                                              </Col>
                                              <Col span={4}>
                                                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                                  <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                                    <Switch size="small" />
                                                  </Form.Item>
                                                  <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} />
                                                </div>
                                              </Col>
                                            </Row>
                                          ))}
                                          <Button
                                            type="dashed"
                                            onClick={() => add(layerEnabled
                                              ? { max_pixels_wan: 261, rate: 0.30, layer_rate: 0.15, enabled: true }
                                              : { max_pixels_wan: 261, rate: 0.30, enabled: true }
                                            )}
                                            block
                                            icon={<PlusOutlined />}
                                            style={{ marginTop: 8, height: 40 }}
                                          >
                                            增加一个像素阶梯
                                          </Button>
                                        </>
                                      )}
                                    </Form.List>
                                  </Form.Item>
                                </RuleContainer>
                              );
                            }}
                          </Form.Item>
                        );
                      }

                      if (rule === 'vidu_image') {
                        // 腾讯云 Vidu 图片精确查表
                        const viduImgAttrs = ['text', 'img2img', 'ref_1_3', 'ref_4_7'];
                        const viduImgRes = ['1k', '2k', '4k'];
                        const viduImgAttrLabels: Record<string, string> = { text: '文生图', img2img: '图生图', ref_1_3: '参考生图 2~3张', ref_4_7: '参考生图 4~7张' };
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="Vidu 图片价格表"
                            description="适用腾讯云 Vidu 等生图模型，按文生图/图生图/参考生图及分辨率组合查表计费。"
                          >
                            <div style={{ padding: '12px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8 }}>
                              <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 12 }}>精确价格表 (单价/张)</Text>
                              {viduImgAttrs.map(attr => viduImgRes.map(res => {
                                const key = `${attr}|${res}`;
                                const isDisabled = viduImageDisabledKeys.includes(key);
                                return (
                                  <PriceTableRow
                                    key={key}
                                    label={`${viduImgAttrLabels[attr] || attr} / ${res}`}
                                    value={viduImagePriceTable[key] ?? 0}
                                    isDisabled={isDisabled}
                                    step={0.01}
                                    addonAfter="/张"
                                    onChange={(val) => setViduImagePriceTable(prev => ({ ...prev, [key]: val }))}
                                    onActiveChange={(checked) => setViduImageDisabledKeys(prev => checked ? prev.filter(k => k !== key) : [...prev, key])}
                                  />
                                );
                              })).flat()}
                            </div>
                          </RuleContainer>
                        );
                      }

                      const ruleLabels: Record<string, string> = {
                        fixed: '固定费率配置',
                        per_image: '按张收费配置',
                        characters: '字符计费配置'
                      };
                      return (
                        <RuleContainer isLight={_isLight} title={ruleLabels[rule] || '常规计费配置'}>
                          <Row gutter={16}>
                            <Col span={12}>
                              <Form.Item name="fixed_rate" label={t('models.fixed_rate')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                                <InputNumber style={{ width: '100%' }} precision={6} addonAfter={rule === 'per_image' ? "/ 张" : rule === 'characters' ? "/ 万字符" : "/ Request"} />
                              </Form.Item>
                            </Col>
                            {rule === 'per_image' && (
                              <>
                                <Col span={6}>
                                  <Form.Item name="prompt_extend_multiplier" label="提示词扩写倍率" tooltip="当请求开启 prompt_extend 时，单价将乘以该倍率 (默认 1.0)" style={{ marginBottom: 0 }}>
                                    <InputNumber style={{ width: '100%' }} precision={2} step={0.1} min={0} />
                                  </Form.Item>
                                </Col>
                                <Col span={6}>
                                  <Form.Item name="image_ref_multiplier" label="有图倍率" tooltip="当请求包含参考图（图生图）时，单价将乘以该倍率 (默认 1.0，不生效)" style={{ marginBottom: 0 }}>
                                    <InputNumber style={{ width: '100%' }} precision={2} step={0.1} min={0} />
                                  </Form.Item>
                                </Col>
                              </>
                            )}
                          </Row>
                        </RuleContainer>
                      );
                    }}
                  </Form.Item>
                </>
              )}

              {billingType === 'duration' && (
                <>
                  <Form.Item name="billing_rule" label="时长计费子模式配置" initialValue="standard" rules={[{ required: true, message: '请选择时长计费子模式' }]}>
                    <Radio.Group optionType="button" buttonStyle="solid">
                      <Radio value="standard">固定时长</Radio>
                      <Radio value="video_resolution">分辨率阶梯</Radio>
                      <Radio value="minimax_h3">MiniMax (秒价+参考图)</Radio>
                      <Radio value="video_seconds_io">双向秒价 + 参考图</Radio>
                      <Radio value="video_seconds_ref">双向秒价 + 参考视频</Radio>
                      <Radio value="fal_ref_video">fal H3-MAX</Radio>
                      <Radio value="video_quality">画质与帧率</Radio>
                      <Radio value="kling_video">可灵视频</Radio>
                      <Radio value="vidu_video">Vidu 视频</Radio>
                      {(hasVolcengineEnhance || editingItem?.billing_rule === 'volc_enhance_cascade') && (
                        <Radio value="volc_enhance_cascade">火山级联封装</Radio>
                      )}
                    </Radio.Group>
                  </Form.Item>

                  <Form.Item noStyle shouldUpdate={(prev, curr) => prev.billing_rule !== curr.billing_rule}>
                    {({ getFieldValue }) => {
                      const rule = getFieldValue('billing_rule');

                      if (rule === 'video_quality') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="画质与帧率阶梯配置"
                            description="按分辨率规格与帧率（≤30fps / >30fps）阶梯匹配秒单价的视频模型。"
                          >
                            <Form.List name="pricing_tiers" initialValue={[]}>
                              {(fields, { add, remove }) => (
                                <>
                                  <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                    <Col span={8}>分辨率规格 (如: 1080p)</Col>
                                    <Col span={7}>帧率范围</Col>
                                    <Col span={6}>秒单价</Col>
                                    <Col span={3}>状态</Col>
                                  </Row>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                      <Col span={8}>
                                        <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                          <Input placeholder="如: 1080p, 2k, 4k" style={{ width: '100%' }} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'fps_range']} rules={[{ required: true }]} noStyle>
                                          <Select placeholder="选择帧率范围" style={{ width: '100%' }}>
                                            <Select.Option value="<=30">≤30fps</Select.Option>
                                            <Select.Option value=">30">{">"}30fps</Select.Option>
                                          </Select>
                                        </Form.Item>
                                      </Col>
                                      <Col span={6}>
                                        <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder="秒单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={3}>
                                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                          <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                          <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                        </div>
                                      </Col>
                                    </Row>
                                  ))}
                                  <Button type="dashed" onClick={() => add({ resolution: '1080p', fps_range: '<=30', rate: 0, enabled: true })} block icon={<PlusOutlined />} style={{ marginTop: 8, height: '40px' }}>
                                    添加画质阶梯
                                  </Button>
                                </>
                              )}
                            </Form.List>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'video_resolution') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="分辨率阶梯计费配置"
                            description="适用阿里 Wan 等按成片分辨率（如 720p、1080p、4k）阶梯计费的视频模型，按分辨率匹配秒单价。"
                          >
                            <Form.List name="pricing_tiers" initialValue={[]}>
                              {(fields, { add, remove }) => (
                                <>
                                  <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                    <Col span={7}>分辨率</Col>
                                    <Col span={7}>秒单价</Col>
                                    <Col span={5}>状态</Col>
                                  </Row>
                                  {fields.map(({ key, name, ...restField }) => (
                                    <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                          <Input placeholder="如: 720p" style={{ width: '100%' }} />
                                        </Form.Item>
                                      </Col>
                                      <Col span={7}>
                                        <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                          <InputNumber placeholder="秒单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={5}>
                                        <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                          <Switch size="small" />
                                        </Form.Item>
                                      </Col>
                                      <Col span={2} style={{ textAlign: 'right' }}>
                                        <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} />
                                      </Col>
                                    </Row>
                                  ))}
                                  <Button type="dashed" onClick={() => add({ resolution: '', rate: 0, enabled: true })} block icon={<PlusOutlined />} style={{ marginTop: 8, height: '40px' }}>
                                    增加一个视频分辨率价格档位
                                  </Button>
                                </>
                              )}
                            </Form.List>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'minimax_h3') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="MiniMax 视频 (秒价+参考图)"
                            description="适用 MiniMax 视频系列模型，按总秒数 × 分辨率秒单价 + 超额输入图（超出免费张数后按张累加）计费。"
                          >
                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={12}>
                                <Form.Item name="prompt_rate" label="输入图额外单价" rules={[{ required: true, message: '请输入输入图额外单价' }]} style={{ marginBottom: 0 }}>
                                  <InputNumber placeholder="超出免费张数后每张价格" style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/张`} />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item
                                  name="free_image_count"
                                  label="输入图免费张数"
                                  tooltip="不超过该数量的输入参考图不计费；默认 5 张"
                                  initialValue={5}
                                  rules={[{ required: true, message: '请输入免费张数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="张" />
                                </Form.Item>
                              </Col>
                            </Row>

                            <Form.Item label="视频分辨率秒单价" required style={{ marginBottom: 0 }}>
                              <Form.List name="pricing_tiers" initialValue={[]}>
                                {(fields, { add, remove }) => (
                                  <>
                                    <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                      <Col span={7}>分辨率</Col>
                                      <Col span={7}>秒单价</Col>
                                      <Col span={5}>状态</Col>
                                    </Row>
                                    {fields.map(({ key, name, ...restField }) => (
                                      <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                        <Col span={7}>
                                          <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                            <Input placeholder="如: 2k / 768p" style={{ width: '100%' }} />
                                          </Form.Item>
                                        </Col>
                                        <Col span={7}>
                                          <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                            <InputNumber placeholder="秒单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                          </Form.Item>
                                        </Col>
                                        <Col span={5}>
                                          <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                        </Col>
                                        <Col span={2} style={{ textAlign: 'right' }}>
                                          <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} />
                                        </Col>
                                      </Row>
                                    ))}
                                    <Button
                                      type="dashed"
                                      onClick={() => add({ resolution: '2k', rate: 0.8, enabled: true })}
                                      block
                                      icon={<PlusOutlined />}
                                      style={{ marginTop: 8, height: '40px' }}
                                    >
                                      增加一个分辨率价格档位
                                    </Button>
                                  </>
                                )}
                              </Form.List>
                            </Form.Item>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'video_seconds_io') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="双向秒价 + 参考图计费配置"
                            description="适用阿里 Wan 等输入与输出秒价分离的视频模型，输入参考时长与生成时长分别按秒计费，超额输入图按张累加。"
                          >
                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={12}>
                                <Form.Item name="prompt_rate" label="输入图额外单价" rules={[{ required: true, message: '请输入输入图额外单价' }]} style={{ marginBottom: 0 }}>
                                  <InputNumber placeholder="超出免费张数后每张价格" style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/张`} />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item
                                  name="free_image_count"
                                  label="输入图免费张数"
                                  tooltip="不超过该数量的输入参考图不计费；默认 5 张"
                                  initialValue={5}
                                  rules={[{ required: true, message: '请输入免费张数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="张" />
                                </Form.Item>
                              </Col>
                            </Row>

                            <Form.Item label="视频分辨率秒单价（输入参考 / 输出生成）" required style={{ marginBottom: 0 }}>
                              <Form.List name="pricing_tiers" initialValue={[]}>
                                {(fields, { add, remove }) => (
                                  <>
                                    <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                      <Col span={5}>分辨率</Col>
                                      <Col span={7}>输入秒单价</Col>
                                      <Col span={7}>生成秒单价</Col>
                                      <Col span={5}>状态</Col>
                                    </Row>
                                    {fields.map(({ key, name, ...restField }) => (
                                      <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                        <Col span={5}>
                                          <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                            <Input placeholder="如: 2k / 768p" style={{ width: '100%' }} />
                                          </Form.Item>
                                        </Col>
                                        <Col span={7}>
                                          <Form.Item {...restField} name={[name, 'input_rate']} rules={[{ required: true }]} noStyle>
                                            <InputNumber placeholder="输入单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                          </Form.Item>
                                        </Col>
                                        <Col span={7}>
                                          <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                            <InputNumber placeholder="生成单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                          </Form.Item>
                                        </Col>
                                        <Col span={5}>
                                          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                            <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                              <Switch size="small" />
                                            </Form.Item>
                                            <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                          </div>
                                        </Col>
                                      </Row>
                                    ))}
                                    <Button
                                      type="dashed"
                                      onClick={() => add({ resolution: '2k', input_rate: 0, rate: 0.8, enabled: true })}
                                      block
                                      icon={<PlusOutlined />}
                                      style={{ marginTop: 8, height: '40px' }}
                                    >
                                      增加一个分辨率价格档位
                                    </Button>
                                  </>
                                )}
                              </Form.List>
                            </Form.Item>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'video_seconds_ref') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="双向秒价 + 参考视频计费配置"
                            description="适用 Vega、可灵、Wan 等区分有无参考视频的模型。支持开启或关闭参考视频区分，开启后可分别配置无参考与有参考的秒单价，超额输入图按张累加。"
                          >
                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={8}>
                                <Form.Item name="prompt_rate" label="输入图额外单价" rules={[{ required: true, message: '请输入输入图额外单价' }]} style={{ marginBottom: 0 }}>
                                  <InputNumber placeholder="超出免费张数后每张价格" style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/张`} />
                                </Form.Item>
                              </Col>
                              <Col span={8}>
                                <Form.Item
                                  name="free_image_count"
                                  label="输入图免费张数"
                                  tooltip="不超过该数量的输入参考图不计费；默认 5 张"
                                  initialValue={5}
                                  rules={[{ required: true, message: '请输入免费张数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="张" />
                                </Form.Item>
                              </Col>
                              <Col span={8}>
                                <Form.Item
                                  name="enable_video_ref"
                                  label="区分有无参考视频定价"
                                  tooltip="开启后可分别设置无参考视频与有参考视频的秒单价；关闭则使用统一的双向秒单价"
                                  valuePropName="checked"
                                  initialValue={true}
                                  style={{ marginBottom: 0 }}
                                >
                                  <Switch checkedChildren="已开启" unCheckedChildren="已关闭" />
                                </Form.Item>
                              </Col>
                            </Row>

                            <Form.Item
                              noStyle
                              shouldUpdate={(prev, curr) => prev.enable_video_ref !== curr.enable_video_ref}
                            >
                              {({ getFieldValue: getRefVal }) => {
                                const isRefAware = getRefVal('enable_video_ref') !== false;
                                return (
                                  <Form.Item
                                    label={isRefAware ? "视频分辨率秒单价 (区分有/无参考视频)" : "视频分辨率秒单价（输入参考 / 输出生成）"}
                                    required
                                    style={{ marginBottom: 0 }}
                                  >
                                    <Form.List name="pricing_tiers" initialValue={[]}>
                                      {(fields, { add, remove }) => (
                                        <>
                                          {isRefAware ? (
                                            <>
                                              <Row gutter={12} style={{ marginBottom: 8, opacity: 0.7, fontSize: 12, fontWeight: 500 }}>
                                                <Col span={6}>分辨率</Col>
                                                <Col span={5}>无参考-生成单价</Col>
                                                <Col span={5}>有参考-输入单价</Col>
                                                <Col span={5}>有参考-生成单价</Col>
                                                <Col span={3}>状态/操作</Col>
                                              </Row>
                                              {fields.map(({ key, name, ...restField }) => (
                                                <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                                  <Col span={6}>
                                                    <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true, message: '请填分辨率' }]} noStyle>
                                                      <Input placeholder="如: 720p" style={{ width: '100%' }} />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={5}>
                                                    <Form.Item {...restField} name={[name, 'rate_no_ref']} rules={[{ required: true, message: '请填无参考生成单价' }]} noStyle>
                                                      <InputNumber placeholder="生成单价" style={{ width: '100%' }} precision={6} min={0} addonAfter="/秒" />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={5}>
                                                    <Form.Item {...restField} name={[name, 'input_rate_ref']} rules={[{ required: true, message: '请填有参考输入单价' }]} noStyle>
                                                      <InputNumber placeholder="输入单价" style={{ width: '100%' }} precision={6} min={0} addonAfter="/秒" />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={5}>
                                                    <Form.Item {...restField} name={[name, 'rate_ref']} rules={[{ required: true, message: '请填有参考生成单价' }]} noStyle>
                                                      <InputNumber placeholder="生成单价" style={{ width: '100%' }} precision={6} min={0} addonAfter="/秒" />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={3}>
                                                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                                      <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                                        <Switch size="small" />
                                                      </Form.Item>
                                                      <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                                    </div>
                                                  </Col>
                                                </Row>
                                              ))}
                                              <Button
                                                type="dashed"
                                                onClick={() => add({
                                                  resolution: '720p',
                                                  rate_no_ref: 1.188,
                                                  input_rate_ref: 0.74,
                                                  rate_ref: 0.74,
                                                  rate: 1.188,
                                                  input_rate: 0.74,
                                                  enabled: true
                                                })}
                                                block
                                                icon={<PlusOutlined />}
                                                style={{ marginTop: 8, height: '40px' }}
                                              >
                                                增加一个分辨率价格档位
                                              </Button>
                                            </>
                                          ) : (
                                            <>
                                              <Row gutter={12} style={{ marginBottom: 8, opacity: 0.5, fontSize: 12 }}>
                                                <Col span={6}>分辨率</Col>
                                                <Col span={7}>输入秒单价</Col>
                                                <Col span={7}>生成秒单价</Col>
                                                <Col span={4}>状态</Col>
                                              </Row>
                                              {fields.map(({ key, name, ...restField }) => (
                                                <Row key={key} gutter={12} align="middle" style={{ marginBottom: 12 }}>
                                                  <Col span={6}>
                                                    <Form.Item {...restField} name={[name, 'resolution']} rules={[{ required: true }]} noStyle>
                                                      <Input placeholder="如: 720p" style={{ width: '100%' }} />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={7}>
                                                    <Form.Item {...restField} name={[name, 'input_rate']} rules={[{ required: true }]} noStyle>
                                                      <InputNumber placeholder="输入单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={7}>
                                                    <Form.Item {...restField} name={[name, 'rate']} rules={[{ required: true }]} noStyle>
                                                      <InputNumber placeholder="生成单价" style={{ width: '100%' }} precision={6} addonAfter="/秒" />
                                                    </Form.Item>
                                                  </Col>
                                                  <Col span={4}>
                                                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                                      <Form.Item {...restField} name={[name, 'enabled']} valuePropName="checked" style={{ marginBottom: 0 }}>
                                                        <Switch size="small" />
                                                      </Form.Item>
                                                      <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(name)} style={{ marginLeft: 8 }} />
                                                    </div>
                                                  </Col>
                                                </Row>
                                              ))}
                                              <Button
                                                type="dashed"
                                                onClick={() => add({ resolution: '720p', input_rate: 0, rate: 0.8, enabled: true })}
                                                block
                                                icon={<PlusOutlined />}
                                                style={{ marginTop: 8, height: '40px' }}
                                              >
                                                增加一个分辨率价格档位
                                              </Button>
                                            </>
                                          )}
                                        </>
                                      )}
                                    </Form.List>
                                  </Form.Item>
                                );
                              }}
                            </Form.Item>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'fal_ref_video') {
                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="fal H3-MAX 计费配置"
                            description="适用 fal.ai 代理的 MiniMax 视频模型，按成片秒数计费，参考素材（图/音/视）共享 Token 池超额计费。"
                          >
                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={12}>
                                <Form.Item
                                  name="duration_rate"
                                  label="成片生成秒单价"
                                  tooltip="每秒成片生成费用（官方原价 0.08 美元/秒，需自行折算为系统货币单位）"
                                  initialValue={0.08}
                                  rules={[{ required: true, message: '请输入成片生成秒单价' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/秒`} />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item
                                  name="free_ref_tokens"
                                  label="参考素材免费 Token 额度"
                                  tooltip="请求中所有图片、视频、音频参考素材共同折算为 Token，该额度内不计费，默认 4096"
                                  initialValue={4096}
                                  rules={[{ required: true, message: '请输入免费 Token 额度' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="Token" />
                                </Form.Item>
                              </Col>
                            </Row>

                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={12}>
                                <Form.Item
                                  name="ref_token_rate_per_1k"
                                  label="参考素材超额单价"
                                  tooltip="超出免费额度后，每 1000 个参考 Token 的价格（官方原价 0.02 美元/千Token，需自行折算为系统货币单位）"
                                  initialValue={0.02}
                                  rules={[{ required: true, message: '请输入超额单价' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={6} min={0} addonAfter={`${currencyUnit}/千Token`} />
                                </Form.Item>
                              </Col>
                              <Col span={12}>
                                <Form.Item
                                  name="image_tokens_default"
                                  label="默认图片 Token 数"
                                  tooltip="每张 1024x1024 参考图片折算 Token 数，默认 1024"
                                  initialValue={1024}
                                  rules={[{ required: true, message: '请输入默认图片 Token 数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} step={1} addonAfter="Token/张" />
                                </Form.Item>
                              </Col>
                            </Row>

                            <Row gutter={16} style={{ marginBottom: 16 }}>
                              <Col span={8}>
                                <Form.Item
                                  name="video_ref_tokens_480p"
                                  label="480p 参考视频每秒 Token"
                                  tooltip="生成 480p 分辨率时，参考视频每秒折算的 Token 数，默认 2886"
                                  initialValue={2886}
                                  rules={[{ required: true, message: '请输入 Token 数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} addonAfter="Token/s" />
                                </Form.Item>
                              </Col>
                              <Col span={8}>
                                <Form.Item
                                  name="video_ref_tokens_768p"
                                  label="768p 参考视频每秒 Token"
                                  tooltip="生成 768p 分辨率时，参考视频每秒折算的 Token 数，默认 7459"
                                  initialValue={7459}
                                  rules={[{ required: true, message: '请输入 Token 数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} addonAfter="Token/s" />
                                </Form.Item>
                              </Col>
                              <Col span={8}>
                                <Form.Item
                                  name="audio_ref_tokens_per_sec"
                                  label="参考音频每秒 Token"
                                  tooltip="参考音频每秒折算的 Token 数，默认 80"
                                  initialValue={80}
                                  rules={[{ required: true, message: '请输入 Token 数' }]}
                                  style={{ marginBottom: 0 }}
                                >
                                  <InputNumber style={{ width: '100%' }} precision={0} min={0} addonAfter="Token/s" />
                                </Form.Item>
                              </Col>
                            </Row>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'kling_video') {
                        return (
                          <Form.Item noStyle dependencies={['kling_use_price_table', 'kling_enable_mode', 'kling_enable_sound', 'kling_enable_video_ref']}>
                            {({ getFieldValue }) => {
                              const usePT = getFieldValue('kling_use_price_table');
                              const eMode = getFieldValue('kling_enable_mode') !== false;
                              const eSound = getFieldValue('kling_enable_sound') !== false;
                              const eVideo = getFieldValue('kling_enable_video_ref') === true;

                              return (
                                <RuleContainer
                                  isLight={_isLight}
                                  title="可灵视频计费配置"
                                  extra={
                                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                      <Text style={{ fontSize: 12 }}>精确查表模式</Text>
                                      <Form.Item name="kling_use_price_table" valuePropName="checked" style={{ marginBottom: 0 }}>
                                        <Switch size="small" />
                                      </Form.Item>
                                    </div>
                                  }
                                  description="适用快手可灵系列模型，支持精确查表或倍率模式，区分画质规格、有声/无声及参考视频。"
                                >
                                  {usePT ? (
                                    <>
                                      {/* 维度开关：控制精确价格表的维度组合 */}
                                      <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', marginBottom: 16, padding: '12px 16px', background: _isLight ? '#f5f5f5' : 'rgba(255,255,255,0.04)', borderRadius: 8 }}>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                          <Text style={{ fontSize: 12 }}>生成模式区分计费</Text>
                                          <Form.Item name="kling_enable_mode" valuePropName="checked" style={{ margin: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                        </div>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                          <Text style={{ fontSize: 12 }}>有声/无声区分计费</Text>
                                          <Form.Item name="kling_enable_sound" valuePropName="checked" style={{ margin: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                        </div>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                          <Text style={{ fontSize: 12 }}>参考视频区分计费</Text>
                                          <Form.Item name="kling_enable_video_ref" valuePropName="checked" style={{ margin: 0 }}>
                                            <Switch size="small" />
                                          </Form.Item>
                                        </div>
                                      </div>

                                      <div style={{ padding: '12px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8 }}>
                                        <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 12 }}>精确价格表 (单价/秒)</Text>
                                        {(() => {
                                          const modes = eMode ? ['std', 'pro', '2k', '4k'] : ['std'];
                                          const sounds = eSound ? ['off', 'on'] : ['off'];
                                          const videos = eVideo ? ['no', 'yes'] : ['no'];
                                          const modeLabels: Record<string, string> = { std: '标准', pro: '高品质', '2k': '2k', '4k': '4k' };
                                          const soundLabels: Record<string, string> = { off: '无声', on: '有声' };
                                          const videoLabels: Record<string, string> = { no: '无参考视频', yes: '有参考视频' };

                                          return modes.map(m => sounds.map(s => videos.map(v => {
                                            const key = `${m}|${s}|${v}`;
                                            const dimLabels = [modeLabels[m] || m];
                                            if (eSound) dimLabels.push(soundLabels[s] || s);
                                            if (eVideo) dimLabels.push(videoLabels[v] || v);
                                            const isDisabled = klingDisabledKeys.includes(key);
                                            return (
                                              <PriceTableRow
                                                key={key}
                                                label={dimLabels.join(' / ')}
                                                value={klingPriceTable[key] ?? 0}
                                                isDisabled={isDisabled}
                                                precision={2}
                                                step={0.1}
                                                addonAfter="/秒"
                                                onChange={(val) => setKlingPriceTable(prev => ({ ...prev, [key]: val }))}
                                                onActiveChange={(checked) => setKlingDisabledKeys(prev => checked ? prev.filter(k => k !== key) : [...prev, key])}
                                              />
                                            );
                                          }))).flat(2);
                                        })()}
                                      </div>
                                    </>
                                  ) : (
                                    <>
                                      <Form.Item name="duration_rate" label="基准秒单价 (倍率模式基准)" rules={[{ required: true }]} style={{ marginBottom: 16 }}>
                                        <InputNumber style={{ width: '200px' }} precision={6} addonAfter="/ s" />
                                      </Form.Item>
                                      <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 8 }}>生成模式 (mode) 倍率</Text>
                                      <Row gutter={16} style={{ marginBottom: 16 }}>
                                        <Col span={6}><Form.Item name="kling_mode_std" label="std (标准)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                        <Col span={6}><Form.Item name="kling_mode_pro" label="pro (高品质)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                        <Col span={6}><Form.Item name="kling_mode_2k" label="2k (2k)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                        <Col span={6}><Form.Item name="kling_mode_4k" label="4k (4k)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                      </Row>
                                      <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 8 }}>声音 (sound) 倍率</Text>
                                      <Row gutter={16} style={{ marginBottom: 16 }}>
                                        <Col span={12}><Form.Item name="kling_sound_off" label="off (无声)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                        <Col span={12}><Form.Item name="kling_sound_on" label="on (有声)" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                      </Row>
                                      <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 8 }}>参考视频 倍率</Text>
                                      <Row gutter={16}>
                                        <Col span={12}><Form.Item name="kling_video_ref_no" label="无参考视频" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                        <Col span={12}><Form.Item name="kling_video_ref_yes" label="有参考视频" style={{ marginBottom: 0 }}><InputNumber style={{ width: '100%' }} precision={2} step={0.1} addonAfter="x" /></Form.Item></Col>
                                      </Row>
                                    </>
                                  )}
                                </RuleContainer>
                              );
                            }}
                          </Form.Item>
                        );
                      }

                      if (rule === 'vidu_video') {
                        const activeLabel = VIDU_VIDEO_ATTRS.find(a => a.value === viduActiveAttr)?.label || '文生视频';

                        // 一键同步：将当前选中的分类价格复制到所有其他生成类型
                        const copyCurrentToAll = () => {
                          setViduVideoPriceTable(prev => {
                            const next = { ...prev };
                            VIDU_VIDEO_RESOLUTIONS.forEach(r => {
                              const val = prev[`${viduActiveAttr}|${r}`] ?? 0;
                              VIDU_VIDEO_ATTRS.forEach(a => {
                                next[`${a.value}|${r}`] = val;
                              });
                            });
                            return next;
                          });
                          message.success(`已将【${activeLabel}】的 6 档分辨率价格同步至全部生成类型`);
                        };

                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="Vidu 视频价格表"
                            description="适用 Vidu 官方开放平台与腾讯云系列视频模型，按生成类型与分辨率组合查表计费，支持错峰折扣。"
                          >
                            <Row gutter={16} align="middle" style={{ marginBottom: 12 }}>
                              <Col>
                                <Form.Item name="vidu_offpeak_discount" label="错峰折扣率" initialValue={1.0} style={{ marginBottom: 0 }}>
                                  <InputNumber style={{ width: '160px' }} precision={2} step={0.1} min={0} max={1} addonAfter="x" />
                                </Form.Item>
                              </Col>
                              <Col>
                                <Text type="secondary" style={{ fontSize: '12px' }}>
                                  💡 传入 off_peak: true 或 service_tier: flex 时自动执行错峰折扣；未声明分辨率默认按 540p 计费
                                </Text>
                              </Col>
                            </Row>

                            <div style={{ padding: '12px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8 }}>
                              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
                                <Segmented
                                  value={viduActiveAttr}
                                  onChange={(val) => setViduActiveAttr(val as string)}
                                  options={VIDU_VIDEO_ATTRS}
                                />
                                <Button
                                  type="dashed"
                                  size="small"
                                  icon={<CopyOutlined />}
                                  onClick={copyCurrentToAll}
                                >
                                  将当前【{activeLabel}】价格同步至其他类型
                                </Button>
                              </div>

                              {VIDU_VIDEO_RESOLUTIONS.map(res => {
                                const key = `${viduActiveAttr}|${res}`;
                                const isDisabled = viduVideoDisabledKeys.includes(key);
                                return (
                                  <PriceTableRow
                                    key={key}
                                    label={`${res} ${res === '540p' ? '(默认基准)' : ''}`}
                                    value={viduVideoPriceTable[key] ?? 0}
                                    isDisabled={isDisabled}
                                    step={0.01}
                                    addonAfter="/秒"
                                    onChange={(val) => setViduVideoPriceTable(prev => ({ ...prev, [key]: val }))}
                                    onActiveChange={(checked) => setViduVideoDisabledKeys(prev => checked ? prev.filter(k => k !== key) : [...prev, key])}
                                  />
                                );
                              })}
                            </div>
                          </RuleContainer>
                        );
                      }

                      if (rule === 'volc_enhance_cascade') {
                        // 覆盖极速版/标准版/专业版/大模型版全档位，与转发规则及计费查表一致
                        const volcVersions = ['fast', 'standard', 'pro', 'ai'];
                        const volcRes = ['720p', '1080p', '2k', '4k'];
                        const volcInputs = ['no', 'yes'];

                        const volcVersionLabels: Record<string, string> = {
                          fast: '极速版 (fast)',
                          standard: '标准版 (standard)',
                          pro: '专业版 (pro)',
                          ai: '大模型封装版 (ai / vve-gt)',
                        };
                        const volcInputLabels: Record<string, string> = {
                          no: '无视频输入',
                          yes: '有视频输入'
                        };

                        return (
                          <RuleContainer
                            isLight={_isLight}
                            title="火山级联封装价格表"
                            description="适用火山引擎视频封装 (MediaKit 级联)，按极速版/标准版/专业版/大模型版、目标分辨率及是否有输入视频查表计费。"
                          >
                            <div style={{ padding: '12px', background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.02)', borderRadius: 8 }}>
                              <Text strong style={{ fontSize: '13px', display: 'block', marginBottom: 12 }}>精确价格表 (单价/秒)</Text>
                              {volcVersions.map(ver => volcRes.map(res => volcInputs.map(input => {
                                const key = `${ver}|${res}|${input}`;
                                const isDisabled = volcCascadeDisabledKeys.includes(key);
                                return (
                                  <PriceTableRow
                                    key={key}
                                    label={`${volcVersionLabels[ver]} / ${res} / ${volcInputLabels[input]}`}
                                    value={volcCascadePriceTable[key] ?? 0}
                                    isDisabled={isDisabled}
                                    step={0.001}
                                    addonAfter="/秒"
                                    onChange={(val) => setVolcCascadePriceTable(prev => ({ ...prev, [key]: val }))}
                                    onActiveChange={(checked) => setVolcCascadeDisabledKeys(prev => checked ? prev.filter(k => k !== key) : [...prev, key])}
                                  />
                                );
                              }))).flat(2)}
                            </div>
                          </RuleContainer>
                        );
                      }

                      return (
                        <RuleContainer
                          isLight={_isLight}
                          title="固定时长计费配置"
                          description="按任务总秒数 × 固定秒单价计费，不区分分辨率档位。"
                        >
                          <Form.Item name="duration_rate" label={t('models.duration_rate')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                            <InputNumber style={{ width: '100%' }} precision={6} addonAfter="/ s" />
                          </Form.Item>
                        </RuleContainer>
                      );
                    }}
                  </Form.Item>
                </>
              )}


              {/* 按天规划时段倍率：上方默认价 × 当时倍率 */}
              <Form.Item
                noStyle
                dependencies={[
                  ['extended_config', 'enable_time_multipliers'],
                  ['extended_config', 'invert_remainder_multiplier'],
                  ['extended_config', 'time_multipliers'],
                  ['extended_config', 'time_input_tz'],
                ]}
              >
                {({ getFieldValue }) => {
                  const enabled = getFieldValue(['extended_config', 'enable_time_multipliers']);
                  const previewItems = getFieldValue(['extended_config', 'time_multipliers']) || [];
                  const remainderField = getFieldValue(['extended_config', 'invert_remainder_multiplier']);
                  const remainderRaw = remainderField == null || remainderField === '' ? NaN : Number(remainderField);
                  const remainder = Number.isFinite(remainderRaw) && remainderRaw >= 0
                    ? remainderRaw
                    : 1;
                  const timeInputTz = (getFieldValue(['extended_config', 'time_input_tz']) || 'operation') as ScheduleTzMode;
                  const clockTz = scheduleInputTzName(timeInputTz, operationTz, adminTz);
                  return (
                    <RuleContainer
                      isLight={_isLight}
                      title="按天规划时段倍率"
                      extra={
                        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                          <RealTimeClock
                            defaultTz={clockTz}
                            label={timeInputTz === 'admin_display' ? '后台时间' : '运营时间'}
                            enabled={enabled}
                            items={previewItems}
                            remainder={remainder}
                          />
                          <Form.Item name={['extended_config', 'enable_time_multipliers']} valuePropName="checked" style={{ margin: 0 }}>
                            <Switch size="small" />
                          </Form.Item>
                        </div>
                      }
                      description="按所选时钟填时段，保存后按运营时区计费。只取一档，未命中用「未规划」。"
                    >
                      {enabled && (
                        <>
                          <Form.Item name={['extended_config', 'time_input_tz']} initialValue="operation" style={{ marginBottom: 12 }}>
                            <Radio.Group
                              buttonStyle="solid"
                              size="small"
                              onChange={(e) => {
                                const next = e.target.value as ScheduleTzMode;
                                const fromTz = scheduleInputTzName(timeInputTz, operationTz, adminTz);
                                const toTz = scheduleInputTzName(next, operationTz, adminTz);
                                form.setFieldValue(
                                  ['extended_config', 'time_multipliers'],
                                  convertScheduleSlots(previewItems, fromTz, toTz),
                                );
                              }}
                            >
                              <Radio.Button value="operation">运营时区</Radio.Button>
                              <Radio.Button value="admin_display">后台时区</Radio.Button>
                            </Radio.Group>
                          </Form.Item>
                          <div style={{
                            display: 'flex',
                            alignItems: 'flex-start',
                            justifyContent: 'space-between',
                            gap: 16,
                            marginBottom: 16,
                            flexWrap: 'wrap',
                          }}>
                            <div style={{ flex: 1, minWidth: 240 }}>
                              <Text style={{ fontSize: 13, fontWeight: 500 }}>未规划时段倍率</Text>
                              <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 4 }}>
                                当天有规划但没命中时用。1.00 = 默认价，0 = 免费。
                              </Text>
                            </div>
                            <Form.Item
                              name={['extended_config', 'invert_remainder_multiplier']}
                              style={{ margin: 0 }}
                              rules={[{ required: true, message: '未规划倍率' }]}
                            >
                              <MultiplierInput
                                style={{ width: 148 }}
                                addonBefore="未规划"
                              />
                            </Form.Item>
                          </div>
                          <Form.List name={['extended_config', 'time_multipliers']} initialValue={[]}>
                            {(fields, { add, remove }) => (
                              <>
                                {fields.map(({ key, name: listName, ...restField }) => {
                                  const row = previewItems[listName] || {};
                                  const allDay = isAllDayMultiplier(row);
                                  const ratio = Number(row.multiplier);
                                  const rowDays = normalizeWeekdays(row.days);
                                  const sharesAllDay = rowDays.some((d) =>
                                    (previewItems || []).some((item: any) =>
                                      isAllDayMultiplier(item) && normalizeWeekdays(item.days).includes(d),
                                    ),
                                  );
                                  const selectedRate = Number.isFinite(ratio) ? ratio : 1;
                                  const restRate = remainder;
                                  const daysLabel = isAllWeekdays(row.days)
                                    ? '每天'
                                    : formatWeekdayRange(row.days, WEEKDAY_CHIPS.map((c) => c.l), '每天');
                                  return (
                                    <div
                                      key={key}
                                      style={{
                                        marginBottom: 12,
                                        padding: '12px 12px 8px',
                                        borderRadius: 8,
                                        border: _isLight ? '1px solid #f0f0f0' : '1px solid #303030',
                                      }}
                                    >
                                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 8 }}>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                          <Text style={{ fontSize: 13 }}>{allDay ? '全天' : '时段'}</Text>
                                          <Form.Item
                                            {...restField}
                                            name={[listName, 'all_day']}
                                            valuePropName="checked"
                                            style={{ margin: 0 }}
                                          >
                                            <Switch size="small" checkedChildren="全天" unCheckedChildren="时段" />
                                          </Form.Item>
                                        </div>
                                        <Button
                                          type="text"
                                          danger
                                          icon={<DeleteOutlined />}
                                          onClick={() => remove(listName)}
                                          size="small"
                                        />
                                      </div>
                                      {!allDay && (
                                      <Row gutter={12} align="middle">
                                        <Col xs={24} sm={8}>
                                          <Form.Item
                                            {...restField}
                                            name={[listName, 'start']}
                                            rules={allDay ? [] : [{ required: true, message: '选择开始时间' }]}
                                            style={{ marginBottom: 8 }}
                                          >
                                            <TimePicker
                                              placeholder="开始时间"
                                              format="HH:mm"
                                              style={{ width: '100%' }}
                                              allowClear={false}
                                              needConfirm={false}
                                            />
                                          </Form.Item>
                                        </Col>
                                        <Col xs={0} sm={1} style={{ textAlign: 'center', marginBottom: 8 }}>
                                          <Text style={{ opacity: 0.5 }}>至</Text>
                                        </Col>
                                        <Col xs={24} sm={8}>
                                          <Form.Item
                                            {...restField}
                                            name={[listName, 'end']}
                                            rules={allDay ? [] : [{ required: true, message: '选择结束时间' }]}
                                            style={{ marginBottom: 8 }}
                                          >
                                            <TimePicker
                                              placeholder="结束时间"
                                              format="HH:mm"
                                              style={{ width: '100%' }}
                                              allowClear={false}
                                              needConfirm={false}
                                            />
                                          </Form.Item>
                                        </Col>
                                        <Col xs={24} sm={7}>
                                          <Form.Item
                                            {...restField}
                                            name={[listName, 'multiplier']}
                                            rules={[{ required: true, message: '倍率' }]}
                                            style={{ marginBottom: 8 }}
                                            initialValue={1.0}
                                          >
                                            <MultiplierInput
                                              placeholder="倍率"
                                              style={{ width: '100%' }}
                                            />
                                          </Form.Item>
                                        </Col>
                                      </Row>
                                      )}
                                      {allDay && (
                                        <Row gutter={12} align="middle">
                                          <Col xs={24} sm={10}>
                                            <Form.Item
                                              {...restField}
                                              name={[listName, 'multiplier']}
                                              rules={[{ required: true, message: '倍率' }]}
                                              style={{ marginBottom: 8 }}
                                              initialValue={0.6}
                                            >
                                              <MultiplierInput
                                                placeholder="全天倍率"
                                                style={{ width: '100%' }}
                                              />
                                            </Form.Item>
                                          </Col>
                                          <Col xs={24} sm={14} style={{ marginBottom: 8 }}>
                                            <Text type="secondary" style={{ fontSize: 12 }}>勾选日 00:00–24:00 均按此倍率</Text>
                                          </Col>
                                        </Row>
                                      )}
                                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap', marginBottom: 4 }}>
                                        <Text type="secondary" style={{ fontSize: 12, flexShrink: 0 }}>星期</Text>
                                        <Form.Item
                                          {...restField}
                                          name={[listName, 'days']}
                                          initialValue={[...ALL_WEEKDAYS]}
                                          style={{ marginBottom: 0, flex: 1 }}
                                        >
                                          <WeekdayPicker />
                                        </Form.Item>
                                      </div>
                                      <Text type="secondary" style={{ fontSize: 12 }}>
                                        {allDay
                                          ? `${daysLabel}全天 ${multiplierPhrase(selectedRate)}；同日更细时段会覆盖，不相乘`
                                          : `${daysLabel}：所选 ${multiplierPhrase(selectedRate)}${sharesAllDay ? '（覆盖全天）' : `，未规划 ${multiplierPhrase(restRate)}`}`}
                                      </Text>
                                    </div>
                                  );
                                })}
                                <div style={{ display: 'flex', gap: 8, marginTop: fields.length > 0 ? 8 : 0 }}>
                                  <Button
                                    type="dashed"
                                    onClick={() => add({ multiplier: 1.0, days: [...ALL_WEEKDAYS], all_day: false })}
                                    block
                                    icon={<PlusOutlined />}
                                  >
                                    添加时段
                                  </Button>
                                  <Button
                                    type="dashed"
                                    onClick={() => {
                                      const used = new Set<number>();
                                      (previewItems || []).forEach((item: any) => {
                                        if (isAllDayMultiplier(item)) {
                                          normalizeWeekdays(item.days).forEach((d) => used.add(d));
                                        }
                                      });
                                      const nextDay = [6, 7, 1, 2, 3, 4, 5].find((d) => !used.has(d)) || 7;
                                      add({ multiplier: nextDay === 7 ? 0.7 : 0.6, days: [nextDay], all_day: true });
                                    }}
                                    block
                                    icon={<PlusOutlined />}
                                  >
                                    添加某天全天
                                  </Button>
                                </div>
                              </>
                            )}
                          </Form.List>
                          {previewItems.length > 0 && (
                            <TimeMultiplierChart items={previewItems} invert remainder={remainder} isLight={_isLight} />
                          )}
                        </>
                      )}
                    </RuleContainer>
                  );
                }}
              </Form.Item>

              {/* 始终启用，隐藏激活状态开关 */}

              <Form.Item style={{ marginTop: 24, textAlign: 'right' }}>
                <Space size="middle">
                  <Button onClick={handleCloseEditor} disabled={saving}>取消</Button>
                  <Button type="primary" loading={saving} onClick={saveAndStay}>保存更新</Button>
                  {editingItem?.is_system === 1 && (
                    <Button danger onClick={handleRestoreDefault} disabled={saving}>恢复默认</Button>
                  )}
                  <Button htmlType="submit" loading={saving} style={{ minWidth: 120 }}>保存退出</Button>
                </Space>
              </Form.Item>
            </Form>
          </div>
        </Card>
      )}

    </>
  );
};

export default BillingRules;
