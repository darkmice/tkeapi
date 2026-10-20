/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import {
  App, Button, Card, Col, DatePicker, Form, Input, InputNumber, Radio, Row, Space, Table, Tag, Tooltip, Typography,
} from 'antd';
import { ArrowLeftOutlined, CopyOutlined, DownloadOutlined } from '@ant-design/icons';
import { useLocation, useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import dayjs from 'dayjs';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { copyToClipboard as copyToClipboardUtil } from '../../utils/clipboard';
import ClassificationFilter from '../../components/Models/ClassificationFilter';
import { unclassifiedChip } from '../../utils/classificationParams';
import type { ClassificationCount, ModelModel, ModelProvider, ModelType } from '../../types';

const { Text } = Typography;

interface GenerateResponse {
  success: boolean;
  count: number;
  codes: string[];
  activity_id?: string;
  description?: string;
  remark?: string;
}

type ModelRow = Pick<
  ModelModel,
  | 'id'
  | 'mid'
  | 'name'
  | 'model_id'
  | 'remark'
  | 'provider_id'
  | 'type_id'
  | 'sort_order'
  | 'site_discount'
  | 'site_discount_enabled'
  | 'global_discount'
  | 'global_discount_enabled'
>;

function formatZhe(d?: number | null, isZh = true) {
  const num = Number(d);
  if (!Number.isFinite(num) || num <= 0 || num >= 1) {
    return isZh ? '原价(1.0)' : '100%(1.0)';
  }
  const zhe = Number((num * 10).toFixed(2));
  return isZh ? `${zhe}折(${Number(num.toFixed(4))})` : `${Math.round(num * 100)}%(${Number(num.toFixed(4))})`;
}

const MODEL_PAGE_SIZE = 20;

function modelProviderKey(model: ModelRow) {
  if (model.provider_id == null || model.provider_id === 0) return 0;
  return model.provider_id;
}

function sortModels(list: ModelRow[], providerOrder: ClassificationCount[]) {
  const rank = new Map<number, number>();
  providerOrder.forEach((item, index) => {
    if (item.id != null) rank.set(item.id, index);
  });
  return [...list].sort((a, b) => {
    const providerDiff = (rank.get(modelProviderKey(a)) ?? 9999) - (rank.get(modelProviderKey(b)) ?? 9999);
    if (providerDiff !== 0) return providerDiff;
    const sortDiff = (b.sort_order ?? 0) - (a.sort_order ?? 0);
    if (sortDiff !== 0) return sortDiff;
    return b.id - a.id;
  });
}

function matchClass(value: number | null | undefined, selected: number | null) {
  if (selected == null) return true;
  if (selected === 0) return value == null || value === 0;
  return value === selected;
}

function classChips(
  scope: ModelRow[],
  catalog: Array<{ id: number; name: string; name_en?: string; logo?: string; sort_order?: number }>,
  pick: (model: ModelRow) => number | null | undefined,
): ClassificationCount[] {
  const counts = new Map<number, number>();
  let unclassified = 0;
  scope.forEach((model) => {
    const id = pick(model);
    if (id == null || id === 0) unclassified += 1;
    else counts.set(id, (counts.get(id) || 0) + 1);
  });
  const byId = new Map(catalog.map((item) => [item.id, item]));
  const chips = Array.from(counts.entries())
    .map(([id, count]) => {
      const item = byId.get(id);
      return {
        id,
        name: item?.name || String(id),
        name_en: item?.name_en,
        logo: item?.logo,
        count,
        sort: item?.sort_order ?? 0,
      };
    })
    .sort((a, b) => b.sort - a.sort || a.name.localeCompare(b.name, 'zh'));
  if (unclassified > 0) chips.push({ ...unclassifiedChip(unclassified), logo: undefined, sort: -1 });
  return chips.map(({ sort: _sort, ...chip }) => chip);
}

const formInitial = {
  quota: 10,
  gift_quota: 0,
  worth_quota: 0 as number | null,
  voucher_discount: 1,
  model_discounts: {} as Record<string, number>,
  count: 3,
  effective_immediately: true,
  permanent: true,
  max_uses_limit: { unlimited: false, count: 1 },
  activity_limit: { unlimited: false, count: 1 },
  code_type: 'balance',
  grant_permanent: true,
  valid_days: 30,
  eligibility_enabled: false,
  condition_match: 0,
  cond_min_balance_on: false,
  cond_min_balance: 10,
  cond_month_spend_on: false,
  cond_month_spend: 10,
  cond_total_spend_on: false,
  cond_total_spend: 10,
  cond_month_recharge_on: false,
  cond_month_recharge: 10,
  cond_total_recharge_on: false,
  cond_total_recharge: 10,
  allowed_models: [] as string[],
  description: '',
  remark: '',
};

function ModelPicker({
  value,
  onChange,
  models,
  providers,
  types,
  loading,
  isZh,
  voucherDiscount,
  modelDiscounts,
  onModelDiscountsChange,
}: {
  value?: string[];
  onChange?: (next: string[]) => void;
  models: ModelRow[];
  providers: ModelProvider[];
  types: ModelType[];
  loading: boolean;
  isZh: boolean;
  voucherDiscount?: number;
  modelDiscounts?: Record<string, number>;
  onModelDiscountsChange?: (next: Record<string, number>) => void;
}) {
  const [keyword, setKeyword] = useState('');
  const [selectedProvider, setSelectedProvider] = useState<number | null>(null);
  const [selectedType, setSelectedType] = useState<number | null>(null);
  const selected = value || [];
  const providerChips = useMemo(
    () => classChips(
      models.filter((model) => matchClass(model.type_id, selectedType)),
      providers,
      (model) => model.provider_id,
    ),
    [models, providers, selectedType],
  );
  const typeChips = useMemo(
    () => classChips(
      models.filter((model) => matchClass(model.provider_id, selectedProvider)),
      types,
      (model) => model.type_id,
    ),
    [models, types, selectedProvider],
  );

  useEffect(() => {
    if (selectedProvider != null && !providerChips.some((item) => item.id === selectedProvider)) {
      setSelectedProvider(null);
    }
  }, [providerChips, selectedProvider]);

  useEffect(() => {
    if (selectedType != null && !typeChips.some((item) => item.id === selectedType)) {
      setSelectedType(null);
    }
  }, [typeChips, selectedType]);

  const q = keyword.trim().toLowerCase();
  const sorted = useMemo(() => sortModels(models.filter((model) => {
    if (!matchClass(model.provider_id, selectedProvider) || !matchClass(model.type_id, selectedType)) return false;
    if (!q) return true;
    return [model.mid, model.name, model.model_id, model.remark || '', String(model.id)]
      .some((part) => part.toLowerCase().includes(q));
  }), providerChips), [models, q, selectedProvider, selectedType, providerChips]);
  const filterKey = `${selectedProvider ?? ''}|${selectedType ?? ''}|${q}`;
  const [visibleCount, setVisibleCount] = useState(MODEL_PAGE_SIZE);
  const [appliedFilter, setAppliedFilter] = useState(filterKey);
  const totalRef = useRef(sorted.length);
  const loadLock = useRef(false);
  if (filterKey !== appliedFilter) {
    setAppliedFilter(filterKey);
    setVisibleCount(MODEL_PAGE_SIZE);
    loadLock.current = false;
  }
  totalRef.current = sorted.length;

  const shown = sorted.slice(0, visibleCount);
  const hasMore = shown.length < sorted.length;

  useEffect(() => {
    loadLock.current = false;
  }, [visibleCount]);

  const onListScroll = (event: React.UIEvent<HTMLDivElement>) => {
    if (!hasMore || loadLock.current) return;
    const root = event.currentTarget;
    if (root.scrollTop + root.clientHeight < root.scrollHeight - 24) return;
    loadLock.current = true;
    setVisibleCount((count) => Math.min(totalRef.current, count + MODEL_PAGE_SIZE));
  };
  const modelKey = (model: ModelRow) => model.mid;
  const selectedMids = useMemo(() => {
    if (!selected.length) return [] as string[];
    if (!models.length) return selected;
    const midSet = new Set(models.map((model) => modelKey(model)));
    const out: string[] = [];
    selected.forEach((raw) => {
      const key = String(raw).trim();
      if (!key) return;
      const resolved = midSet.has(key)
        ? key
        : (models.find((model) => model.model_id === key)?.mid || key);
      if (!out.includes(resolved)) out.push(resolved);
    });
    return out;
  }, [models, selected]);
  useEffect(() => {
    if (!models.length || !selected.length) return;
    if (selectedMids.length !== selected.length || selectedMids.some((mid, idx) => mid !== selected[idx])) {
      onChange?.(selectedMids);
    }
  }, [models, onChange, selected, selectedMids]);
  const selectedMidSet = useMemo(() => new Set(selectedMids), [selectedMids]);
  const picked = useMemo(
    () => models.filter((model) => selectedMidSet.has(modelKey(model))),
    [models, selectedMidSet],
  );

  const syncSelection = (keys: Array<string | number>) => {
    const keySet = new Set(keys.map((key) => String(key)));
    const visibleMids = new Set(shown.map((model) => modelKey(model)));
    const next = new Set<string>();
    models.forEach((model) => {
      const key = modelKey(model);
      if (!visibleMids.has(key)) {
        if (selectedMidSet.has(key)) next.add(key);
        return;
      }
      if (keySet.has(key)) next.add(key);
    });
    onChange?.(Array.from(next));
  };

  const updateModelDiscount = (mid: string, val: number | null) => {
    const next = { ...(modelDiscounts || {}) };
    if (val === null || val === undefined || !Number.isFinite(Number(val)) || Number(val) <= 0) {
      delete next[mid];
    } else {
      next[mid] = Math.min(1, Math.max(0.0001, Number(val)));
    }
    onModelDiscountsChange?.(next);
  };

  const renderModelPricingMeta = (model: ModelRow, overrideVal?: number) => {
    const hasGlobal = model.global_discount_enabled === 1 && model.global_discount != null && model.global_discount > 0 && model.global_discount < 1;
    const globalD = hasGlobal ? Number(model.global_discount) : null;
    const hasFloor = model.site_discount_enabled === 1 && model.site_discount != null && model.site_discount > 0;
    const floorD = hasFloor ? Number(model.site_discount) : null;
    const baseCouponD =
      overrideVal != null && overrideVal > 0 && overrideVal <= 1
        ? overrideVal
        : voucherDiscount != null && voucherDiscount > 0 && voucherDiscount <= 1
          ? Number(voucherDiscount)
          : 1;
    const usesModelPrice = globalD != null && baseCouponD > globalD;
    const preFloorD = usesModelPrice ? globalD : baseCouponD;
    const hitsFloor = floorD != null && preFloorD < floorD;
    const finalD = hitsFloor ? Math.min(1, floorD) : preFloorD;

    return (
      <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 4, fontSize: 11, lineHeight: 1.2 }}>
        {globalD != null ? (
          <Tag color="blue" style={{ margin: 0, padding: '0 4px', fontSize: 10, lineHeight: '16px' }}>
            {isZh ? `模型价 ${formatZhe(globalD, isZh)}` : `Model ${formatZhe(globalD, isZh)}`}
          </Tag>
        ) : (
          <Text type="secondary" style={{ fontSize: 11 }}>{isZh ? '模型原价' : 'Base 1.0'}</Text>
        )}
        {floorD != null && (
          <Tag color="orange" style={{ margin: 0, padding: '0 4px', fontSize: 10, lineHeight: '16px' }}>
            {isZh ? `最低限价 ${formatZhe(floorD, isZh)}` : `Floor ${formatZhe(floorD, isZh)}`}
          </Tag>
        )}
        {baseCouponD < 1 && (
          <Tooltip
            title={
              hitsFloor
                ? (isZh
                  ? `卡券设 ${formatZhe(baseCouponD, isZh)} 低于该模型最低限价，实际兜底按最低限价 ${formatZhe(finalD, isZh)} 结算`
                  : `Coupon ${formatZhe(baseCouponD, isZh)} is below model floor; clamped to ${formatZhe(finalD, isZh)}`)
                : usesModelPrice
                  ? (isZh
                    ? `卡券设 ${formatZhe(baseCouponD, isZh)} 高于该模型折扣价，实际按更优惠的模型折扣 ${formatZhe(finalD, isZh)} 结算`
                    : `Coupon ${formatZhe(baseCouponD, isZh)} is higher than model discount; billed at ${formatZhe(finalD, isZh)}`)
                  : (isZh
                    ? `消耗卡券面值时优先按卡券折扣 ${formatZhe(finalD, isZh)} 结算`
                    : `Billed at coupon discount ${formatZhe(finalD, isZh)} while consuming voucher`)
            }
          >
            <Tag color={hitsFloor ? 'volcano' : usesModelPrice ? 'cyan' : 'green'} style={{ margin: 0, padding: '0 4px', fontSize: 10, lineHeight: '16px', cursor: 'help' }}>
              {hitsFloor
                ? (isZh ? `限价兜底 ${formatZhe(finalD, isZh)}` : `Floor ${formatZhe(finalD, isZh)}`)
                : usesModelPrice
                  ? (isZh ? `按模型价 ${formatZhe(finalD, isZh)}` : `Model ${formatZhe(finalD, isZh)}`)
                  : (isZh ? `生效 ${formatZhe(finalD, isZh)}` : `Eff ${formatZhe(finalD, isZh)}`)}
            </Tag>
          </Tooltip>
        )}
      </div>
    );
  };

  return (
    <div>
      <div className="redemption-model-filter">
        <ClassificationFilter
          providers={providerChips}
          types={typeChips}
          selectedProvider={selectedProvider}
          selectedType={selectedType}
          onProviderChange={setSelectedProvider}
          onTypeChange={setSelectedType}
        />
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: 8, flexWrap: 'wrap', marginBottom: 8 }}>
        <Input.Search
          allowClear
          size="small"
          value={keyword}
          onChange={(e) => setKeyword(e.target.value)}
          placeholder={isZh ? '搜索 MID、名称、模型 ID、备注' : 'Search MID, name, model ID, remark'}
          style={{ width: 320, maxWidth: '100%' }}
        />
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>{isZh ? `已选 ${picked.length} 个` : `${picked.length} selected`}</Text>
          {picked.length > 0 && (
            <Button type="link" size="small" onClick={() => onChange?.([])} style={{ padding: 0, height: 22 }}>
              {isZh ? '清空' : 'Clear'}
            </Button>
          )}
        </div>
      </div>
      {picked.length > 0 && (
        <div
          style={{
            marginBottom: 8,
            maxHeight: 168,
            overflow: 'auto',
            border: '1px solid var(--ant-color-border-secondary)',
            borderRadius: 6,
          }}
        >
          {picked.map((model) => {
            const key = modelKey(model);
            const overrideVal = modelDiscounts?.[key];
            const hasFloor = model.site_discount_enabled === 1 && model.site_discount != null && model.site_discount > 0;
            const floorD = hasFloor ? Math.min(1, Number(model.site_discount)) : null;
            const belowFloor = overrideVal != null && floorD != null && overrideVal + 1e-9 < floorD;
            return (
              <div
                key={key}
                style={{
                  display: 'grid',
                  gridTemplateColumns: '64px minmax(0, 1.1fr) minmax(0, 1.2fr) minmax(0, 0.9fr) minmax(0, 1.3fr) 136px 36px',
                  gap: 8,
                  alignItems: 'center',
                  padding: '3px 8px',
                  lineHeight: '18px',
                  fontSize: 12,
                  borderBottom: '1px solid var(--ant-color-border-secondary)',
                }}
              >
                <Text code style={{ fontSize: 11 }}>{model.mid || '-'}</Text>
                <span className="rg-model-full">{model.name || '-'}</span>
                <span className="rg-model-full" style={{ fontFamily: 'monospace' }}>{model.model_id}</span>
                <span className="rg-model-full" style={{ color: 'var(--ant-color-text-secondary)' }}>
                  {model.remark?.trim() || (isZh ? '无备注' : 'No remark')}
                </span>
                {renderModelPricingMeta(model, overrideVal)}
                <InputNumber
                  size="small"
                  min={floorD ?? 0.01}
                  max={1}
                  step={0.05}
                  precision={4}
                  status={belowFloor ? 'error' : undefined}
                  value={overrideVal ?? null}
                  onChange={(val) => updateModelDiscount(key, val)}
                  onBlur={() => {
                    if (belowFloor && floorD != null) {
                      updateModelDiscount(key, floorD);
                    }
                  }}
                  placeholder={
                    voucherDiscount && voucherDiscount < 1
                      ? (isZh
                        ? `默认 ${floorD != null ? Math.max(voucherDiscount, floorD) : voucherDiscount}`
                        : `Def ${floorD != null ? Math.max(voucherDiscount, floorD) : voucherDiscount}`)
                      : floorD != null
                        ? (isZh ? `专属折扣(≥${floorD})` : `≥${floorD}`)
                        : (isZh ? '专属折扣(0.01~1)' : 'Discount')
                  }
                  style={{ width: '100%' }}
                />
                <Button
                  type="link"
                  size="small"
                  style={{ padding: 0, height: 22 }}
                  onClick={() => onChange?.(selectedMids.filter((mid) => mid !== key))}
                >
                  {isZh ? '移除' : 'Remove'}
                </Button>
              </div>
            );
          })}
        </div>
      )}
      <div className="redemption-model-scroll" onScroll={onListScroll}>
      <Table<ModelRow>
        className="redemption-model-table"
        size="small"
        rowKey={(model) => modelKey(model)}
        loading={loading}
        dataSource={shown}
        pagination={false}
        rowSelection={{
          selectedRowKeys: shown.filter((model) => selectedMidSet.has(modelKey(model))).map((model) => modelKey(model)),
          preserveSelectedRowKeys: true,
          onChange: (keys) => syncSelection(keys as (string | number)[]),
        }}
        columns={[
          {
            title: isZh ? '模型 MID' : 'MID',
            dataIndex: 'mid',
            width: 110,
            render: (mid: string) => <Text code>{mid || '-'}</Text>,
          },
          {
            title: isZh ? '模型名称' : 'Name',
            dataIndex: 'name',
            render: (name: string) => <span className="rg-model-full">{name || '-'}</span>,
          },
          {
            title: isZh ? '模型 ID' : 'Model ID',
            dataIndex: 'model_id',
            render: (modelId: string) => (
              <span className="rg-model-full" style={{ fontFamily: 'monospace' }}>{modelId}</span>
            ),
          },
          {
            title: isZh ? '模型折扣 / 最低限价' : 'Discount / Floor',
            key: 'pricing_meta',
            width: 210,
            render: (_, model) => renderModelPricingMeta(model, modelDiscounts?.[modelKey(model)]),
          },
          {
            title: isZh ? '模型备注' : 'Remark',
            dataIndex: 'remark',
            width: 96,
            ellipsis: true,
            render: (remark?: string | null) => remark?.trim()
              ? <span>{remark.trim()}</span>
              : <Text type="secondary">{isZh ? '无备注' : 'No remark'}</Text>,
          },
        ]}
      />
      </div>
      <div style={{ marginTop: 4, fontSize: 12, lineHeight: '18px', color: 'var(--ant-color-text-secondary)' }}>
        {sorted.length === 0
          ? (isZh ? '没有匹配的模型' : 'No matching models')
          : hasMore
            ? (isZh ? `已加载 ${shown.length} / ${sorted.length}，继续向下滚动` : `${shown.length} / ${sorted.length} loaded, scroll for more`)
            : (isZh ? `共 ${sorted.length} 个模型` : `${sorted.length} models`)}
      </div>
    </div>
  );
}

type UsageLimit = { unlimited: boolean; count: number };

function resolveUsageLimit(value: UsageLimit | undefined): number {
  if (value?.unlimited) return -1;
  const count = Math.trunc(Number(value?.count) || 1);
  return count >= 1 ? count : 1;
}

function UsageLimitInput({
  value,
  onChange,
  isZh,
  max,
}: {
  value?: UsageLimit;
  onChange?: (next: UsageLimit) => void;
  isZh: boolean;
  max: number;
}) {
  const current: UsageLimit = value && value.count >= 1
    ? value
    : { unlimited: !!value?.unlimited, count: 1 };
  const selectCount = () => {
    if (current.unlimited) onChange?.({ unlimited: false, count: current.count });
  };
  return (
    <div className={`rg-usage${current.unlimited ? ' is-unlimited' : ' is-count'}`}>
      <div className="rg-usage-count">
        <InputNumber
          variant="borderless"
          min={1}
          max={max}
          precision={0}
          disabled={current.unlimited}
          value={current.count}
          onChange={(raw) => {
            const count = typeof raw === 'number' && raw >= 1 ? Math.min(max, Math.trunc(raw)) : 1;
            onChange?.({ unlimited: false, count });
          }}
        />
        <span className="rg-usage-unit">{isZh ? '次' : 'times'}</span>
        {current.unlimited && (
          <button type="button" className="rg-usage-mask" aria-label={isZh ? '改为指定次数' : 'Set a limit'} onClick={selectCount} />
        )}
      </div>
      <button
        type="button"
        className="rg-usage-unlimit"
        aria-pressed={current.unlimited}
        onClick={() => onChange?.({ unlimited: true, count: current.count })}
      >
        {isZh ? '不限制' : 'Unlimited'}
      </button>
    </div>
  );
}

interface QuickTimePreset {
  label: string;
  days: number;
  hours: number;
}

function QuickTimeInput({
  value,
  onChange,
  baseTime,
  isZh,
  presets,
  durationLabel,
  disabledDate,
  placeholder,
}: {
  value?: dayjs.Dayjs | null;
  onChange?: (next: dayjs.Dayjs | null) => void;
  baseTime?: dayjs.Dayjs | null;
  isZh: boolean;
  presets: QuickTimePreset[];
  durationLabel: string;
  disabledDate?: (current: dayjs.Dayjs) => boolean;
  placeholder?: string;
}) {
  const [days, setDays] = useState<number | null>(null);
  const [hours, setHours] = useState<number | null>(null);

  useEffect(() => {
    if (!value || !value.isValid()) {
      setDays(null);
      setHours(null);
      return;
    }
    const base = (baseTime && baseTime.isValid() ? baseTime : dayjs()).second(0).millisecond(0);
    const diffMinutes = value.diff(base, 'minute');
    if (diffMinutes >= 0) {
      const d = Math.floor(diffMinutes / 1440);
      const h = Math.round((diffMinutes % 1440) / 60);
      setDays(d);
      setHours(h);
    } else {
      setDays(null);
      setHours(null);
    }
  }, [value, baseTime]);

  const handleDurationChange = (nextD: number | null, nextH: number | null) => {
    setDays(nextD);
    setHours(nextH);
    const d = Math.max(0, Number(nextD ?? 0));
    const h = Math.max(0, Number(nextH ?? 0));
    if (d === 0 && h === 0) {
      onChange?.(null);
      return;
    }
    const base = (baseTime && baseTime.isValid() ? baseTime : dayjs()).second(0).millisecond(0);
    const nextVal = base.add(d, 'day').add(h, 'hour');
    onChange?.(nextVal);
  };

  const handlePickerChange = (date: dayjs.Dayjs | null) => {
    onChange?.(date);
    if (!date || !date.isValid()) {
      setDays(null);
      setHours(null);
      return;
    }
    const base = (baseTime && baseTime.isValid() ? baseTime : dayjs()).second(0).millisecond(0);
    const diffMinutes = date.diff(base, 'minute');
    if (diffMinutes >= 0) {
      const d = Math.floor(diffMinutes / 1440);
      const h = Math.round((diffMinutes % 1440) / 60);
      setDays(d);
      setHours(h);
    } else {
      setDays(null);
      setHours(null);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6, width: '100%' }}>
      <div style={{ display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: 6 }}>
        <Text type="secondary" style={{ fontSize: 12 }}>{durationLabel}</Text>
        <InputNumber
          size="small"
          min={0}
          precision={0}
          value={days ?? undefined}
          onChange={(v) => handleDurationChange(v, hours)}
          placeholder="0"
          style={{ width: 62 }}
        />
        <Text style={{ fontSize: 12 }}>{isZh ? '天' : 'd'}</Text>
        <InputNumber
          size="small"
          min={0}
          precision={0}
          value={hours ?? undefined}
          onChange={(v) => handleDurationChange(days, v)}
          placeholder="0"
          style={{ width: 62 }}
        />
        <Text style={{ fontSize: 12 }}>{isZh ? '小时' : 'h'}</Text>
        <Space size={4} wrap style={{ marginLeft: 4 }}>
          {presets.map((preset) => {
            const isMatch = (days ?? 0) === preset.days && (hours ?? 0) === preset.hours && (days !== null || hours !== null);
            return (
              <Button
                key={preset.label}
                size="small"
                type={isMatch ? 'primary' : 'default'}
                onClick={() => handleDurationChange(preset.days, preset.hours)}
                style={{ fontSize: 12, padding: '0 8px', height: 24, borderRadius: 4 }}
              >
                {preset.label}
              </Button>
            );
          })}
        </Space>
      </div>
      <DatePicker
        showTime={{ format: 'HH:mm' }}
        style={{ width: '100%' }}
        format="YYYY-MM-DD HH:mm"
        disabledDate={disabledDate}
        value={value}
        onChange={handlePickerChange}
        placeholder={placeholder}
      />
    </div>
  );
}

function GrantDurationInput({
  value,
  onChange,
  isZh,
}: {
  value?: number;
  onChange?: (days: number) => void;
  isZh: boolean;
}) {
  const days = typeof value === 'number' && value >= 1 ? Math.trunc(value) : 30;
  const presets = [
    { label: isZh ? '1天' : '1d', days: 1 },
    { label: isZh ? '7天' : '7d', days: 7 },
    { label: isZh ? '30天' : '30d', days: 30 },
    { label: isZh ? '60天' : '60d', days: 60 },
    { label: isZh ? '180天' : '180d', days: 180 },
  ];
  const setDays = (next: number) => {
    const days = Math.min(36500, Math.max(1, Math.trunc(next) || 1));
    onChange?.(days);
  };
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8 }}>
      <InputNumber
        min={1}
        max={36500}
        precision={0}
        value={days}
        onChange={(raw) => setDays(typeof raw === 'number' ? raw : 1)}
        style={{ width: 120 }}
      />
      <span>{isZh ? '天' : 'days'}</span>
      {presets.map((preset) => (
        <Button
          key={preset.days}
          size="small"
          type={days === preset.days ? 'primary' : 'default'}
          onClick={() => setDays(preset.days)}
        >
          {preset.label}
        </Button>
      ))}
    </div>
  );
}

function GateAmount({
  enabled,
  onToggle,
  amount,
  onAmount,
  isZh,
  currencySymbol,
}: {
  enabled: boolean;
  onToggle: (on: boolean) => void;
  amount?: number;
  onAmount: (amount: number) => void;
  isZh: boolean;
  currencySymbol: string;
}) {
  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8 }}>
      <Button size="small" type={enabled ? 'default' : 'primary'} onClick={() => onToggle(false)}>
        {isZh ? '不要求' : 'Off'}
      </Button>
      <Button size="small" type={enabled ? 'primary' : 'default'} onClick={() => onToggle(true)}>
        {isZh ? '要求达到' : 'Require'}
      </Button>
      {enabled && (
        <>
          <InputNumber
            min={0.01}
            precision={2}
            value={amount}
            onChange={(raw) => onAmount(typeof raw === 'number' ? raw : 0.01)}
            style={{ width: 140 }}
          />
          <span>{currencySymbol}</span>
        </>
      )}
    </div>
  );
}

function TwoChoiceButtons<T extends string | boolean | number>({
  value,
  onChange,
  options,
}: {
  value?: T;
  onChange?: (val: T) => void;
  options: Array<{ label: string; value: T }>;
}) {
  const activeValue = value !== undefined ? value : options[0]?.value;
  return (
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      {options.map((opt) => (
        <Button
          key={String(opt.value)}
          type={activeValue === opt.value ? 'primary' : 'default'}
          onClick={() => onChange?.(opt.value)}
        >
          {opt.label}
        </Button>
      ))}
    </div>
  );
}

const RedemptionGenerate: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { message: msgApi } = App.useApp();
  const isZh = i18n.language === 'zh';
  const navigate = useNavigate();
  const location = useLocation();
  const { settings } = useSettingsStore();
  const listUrl = location.pathname.replace(/\/new\/?$/, '') || '/';
  const currencySymbol = settings?.currency?.currency_symbol || '$';
  const [form] = Form.useForm();
  const [submitting, setSubmitting] = useState(false);
  const [models, setModels] = useState<ModelRow[]>([]);
  const [providers, setProviders] = useState<ModelProvider[]>([]);
  const [modelTypes, setModelTypes] = useState<ModelType[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [generated, setGenerated] = useState<GenerateResponse | null>(null);

  const permanent = Form.useWatch('permanent', form);
  const grantPermanent = Form.useWatch('grant_permanent', form);
  const effectiveImmediately = Form.useWatch('effective_immediately', form);
  const startsAt = Form.useWatch('starts_at', form);
  const codeType = Form.useWatch('code_type', form);
  const voucherDiscount = Form.useWatch('voucher_discount', form);
  const modelDiscounts = Form.useWatch('model_discounts', form);

  const baseForExpiry = useMemo(() => {
    if (effectiveImmediately === false && startsAt && dayjs(startsAt).isValid()) {
      return dayjs(startsAt);
    }
    return dayjs();
  }, [effectiveImmediately, startsAt]);

  const commonTimePresets = useMemo(() => [
    { label: isZh ? '12小时' : '12h', days: 0, hours: 12 },
    { label: isZh ? '1天' : '1d', days: 1, hours: 0 },
    { label: isZh ? '7天' : '7d', days: 7, hours: 0 },
    { label: isZh ? '30天' : '30d', days: 30, hours: 0 },
    { label: isZh ? '60天' : '60d', days: 60, hours: 0 },
    { label: isZh ? '180天' : '180d', days: 180, hours: 0 },
  ], [isZh]);

  useEffect(() => {
    let cancelled = false;
    const loadModels = async () => {
      setModelsLoading(true);
      try {
        const [resp, providerResp, typeResp] = await Promise.all([
          request.get('/models') as unknown as Promise<{ data?: ModelRow[] }>,
          request.get('/model-providers') as unknown as Promise<ModelProvider[] | { data?: ModelProvider[] }>,
          request.get('/model-types') as unknown as Promise<ModelType[] | { data?: ModelType[] }>,
        ]);
        if (cancelled) return;
        const seen = new Set<string>();
        setModels((resp?.data || []).filter((model) => {
          const mid = (model.mid || '').trim();
          if (!mid || seen.has(mid)) return false;
          seen.add(mid);
          return true;
        }));
        const providerList = Array.isArray(providerResp) ? providerResp : (providerResp?.data || []);
        const typeList = Array.isArray(typeResp) ? typeResp : (typeResp?.data || []);
        setProviders(providerList);
        setModelTypes(typeList);
      } catch (e) {
        console.error(e);
        if (!cancelled) setModels([]);
      } finally {
        if (!cancelled) setModelsLoading(false);
      }
    };
    loadModels();
    return () => {
      cancelled = true;
    };
  }, []);

  const copyToClipboard = async (text: string) => {
    const ok = await copyToClipboardUtil(text);
    if (ok) {
      msgApi.success(t('common.copied'));
    } else {
      msgApi.error(isZh ? '复制失败，请手动选择复制' : 'Failed to copy, please select manually');
    }
  };

  const downloadCodes = (activityId: string | undefined, codes: string[], slogan?: string) => {
    const header = slogan?.trim() ? `${slogan.trim()}\n\n` : '';
    const blob = new Blob([`${header}${codes.join('\n')}\n`], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `redemption-${activityId || 'codes'}.txt`;
    link.click();
    URL.revokeObjectURL(url);
  };

  const handleCreate = async (values: typeof formInitial & {
    name?: string;
    starts_at?: dayjs.Dayjs;
    expires_at?: dayjs.Dayjs;
  }) => {
    if (!values.permanent && values.expires_at) {
      const end = dayjs(values.expires_at);
      if (!end.isAfter(dayjs())) {
        msgApi.error(isZh ? '到期时间必须晚于当前时间' : 'Expiry must be in the future');
        return;
      }
      if (values.effective_immediately === false && values.starts_at && !dayjs(values.starts_at).isBefore(end)) {
        msgApi.error(isZh ? '开始生效时间必须早于到期时间' : 'Start time must be before the expiry');
        return;
      }
    }
    const gateAmount = (on: boolean, amount: number) => (values.eligibility_enabled && on ? Number(amount) : 0);
    if (values.eligibility_enabled) {
      const picked = [
        values.cond_min_balance_on,
        values.cond_month_spend_on,
        values.cond_total_spend_on,
        values.cond_month_recharge_on,
        values.cond_total_recharge_on,
      ].filter(Boolean).length;
      if (picked === 0) {
        msgApi.error(isZh ? '请至少打开一项兑换条件' : 'Turn on at least one redemption condition');
        return;
      }
    }
    const grantDays = Math.trunc(Number(values.valid_days) || 0);
    if ((values.code_type === 'model_voucher' || values.code_type === 'playground_space') && values.grant_permanent === false) {
      if (grantDays < 1 || grantDays > 36500) {
        msgApi.error(isZh ? '请选择兑换后的使用时长' : 'Choose how long the benefit lasts after redeem');
        return;
      }
    }
    const isBalanceCode = values.code_type !== 'model_voucher' && values.code_type !== 'playground_space';
    const isVoucherCode = values.code_type === 'model_voucher';
    const sysQuota = Number(values.quota || 0);
    const giftQuota = isBalanceCode ? Number(values.gift_quota || 0) : 0;
    if (isBalanceCode && sysQuota <= 0 && giftQuota <= 0) {
      msgApi.error(isZh ? '系统余额面值与赠送金余额面值至少有一项大于 0' : 'At least one of system balance or gift balance must be greater than 0');
      return;
    }
    const rawWorth = Number(values.worth_quota || 0);
    const worthQuota = isVoucherCode && Number.isFinite(rawWorth) && rawWorth > 0 ? rawWorth : 0;
    const rawVoucherDiscount = Number(values.voucher_discount);
    const resolvedVoucherDiscount =
      isVoucherCode && Number.isFinite(rawVoucherDiscount) && rawVoucherDiscount > 0 && rawVoucherDiscount <= 1
        ? rawVoucherDiscount
        : 1;
    const cleanedModelDiscounts: Record<string, number> = {};
    if (isVoucherCode && values.model_discounts) {
      const selectedSet = new Set(Array.isArray(values.allowed_models) ? values.allowed_models : []);
      for (const [k, v] of Object.entries(values.model_discounts)) {
        const key = String(k).trim();
        const num = Number(v);
        if (key && selectedSet.has(key) && Number.isFinite(num) && num > 0 && num <= 1) {
          const m = models.find((item) => item.mid === key || item.model_id === key);
          if (m && m.site_discount_enabled === 1 && m.site_discount != null && m.site_discount > 0) {
            const floor = Math.min(1, Number(m.site_discount));
            if (num + 1e-9 < floor) {
              msgApi.error(
                isZh
                  ? `模型 ${m.name || m.model_id || key} 的专属折扣 (${formatZhe(num, isZh)}) 不能低于其最低限价 (${formatZhe(floor, isZh)})`
                  : `Exclusive discount (${num}) for ${m.name || m.model_id || key} cannot be lower than its floor (${floor})`,
              );
              return;
            }
          }
          cleanedModelDiscounts[key] = num;
        }
      }
    }
    const maxUses = resolveUsageLimit(values.max_uses_limit);
    const activityLimit = resolveUsageLimit(values.activity_limit);
    const allowMultiple = maxUses !== 1;
    const payload = {
      name: values.name?.trim(),
      description: values.description?.trim() || '',
      remark: values.remark?.trim() || '',
      count: values.count,
      quota: sysQuota,
      gift_quota: giftQuota,
      worth_quota: worthQuota,
      voucher_discount: resolvedVoucherDiscount,
      model_discounts: cleanedModelDiscounts,
      permanent: !!values.permanent,
      expires_at: values.permanent
        ? null
        : (values.expires_at ? dayjs(values.expires_at).toISOString() : null),
      effective_immediately: values.effective_immediately !== false,
      starts_at: values.effective_immediately === false && values.starts_at
        ? dayjs(values.starts_at).toISOString()
        : null,
      allow_multiple: allowMultiple,
      max_uses: maxUses,
      per_user_limit: allowMultiple ? -1 : 1,
      per_user_activity_limit: activityLimit,
      code_type: values.code_type === 'model_voucher'
        ? 'model_voucher'
        : values.code_type === 'playground_space'
          ? 'playground_space'
          : 'balance',
      allowed_models: values.code_type === 'model_voucher'
        ? (Array.isArray(values.allowed_models) ? values.allowed_models : [])
        : [],
      valid_days: values.code_type === 'model_voucher' || values.code_type === 'playground_space'
        ? (values.grant_permanent !== false ? 0 : Math.trunc(Number(values.valid_days) || 0))
        : 0,
      condition_match: values.eligibility_enabled && values.condition_match === 1 ? 1 : 0,
      cond_min_balance: gateAmount(values.cond_min_balance_on, values.cond_min_balance),
      cond_month_spend: gateAmount(values.cond_month_spend_on, values.cond_month_spend),
      cond_total_spend: gateAmount(values.cond_total_spend_on, values.cond_total_spend),
      cond_month_recharge: gateAmount(values.cond_month_recharge_on, values.cond_month_recharge),
      cond_total_recharge: gateAmount(values.cond_total_recharge_on, values.cond_total_recharge),
    };
    setSubmitting(true);
    try {
      const resp = await (request.post('/redemptions', payload) as unknown as Promise<GenerateResponse>);
      if (resp.success) {
        msgApi.success(t('common.success'));
        setGenerated({
          ...resp,
          description: resp.description ?? payload.description,
          remark: resp.remark ?? payload.remark,
        });
      }
    } catch (e) {
      console.error(e);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Card variant="borderless" className="redemption-generate" styles={{ body: { padding: 12 } }}>
      <div className="rg-page-head">
        <Button size="small" icon={<ArrowLeftOutlined />} onClick={() => navigate(listUrl)} />
        <div>
          <div className="rg-page-title">{t('redemptions.add')}</div>
          <div className="rg-page-desc">
            {isZh
              ? '填写活动规则后生成兑换码。活动 ID 由系统生成，生成后不可修改。'
              : 'Set the rules, then generate codes. The activity ID is assigned automatically and cannot be changed.'}
          </div>
        </div>
      </div>

      {generated ? (
        <section className="rg-section">
          <div className="rg-section-head">
            <div className="rg-section-title">{t('redemptions.codes_generated')}</div>
            <div className="rg-page-desc">
              {isZh
                ? `活动 ID ${generated.activity_id || '-'} 已生成，共 ${generated.codes.length} 个兑换码。`
                : `Activity ID ${generated.activity_id || '-'} created, ${generated.codes.length} codes.`}
              {generated.remark?.trim()
                ? (isZh ? `（管理员备注：${generated.remark.trim()}）` : ` (Admin remark: ${generated.remark.trim()})`)
                : ''}
            </div>
          </div>
          <div className="rg-section-body">
            {generated.description?.trim() && (
              <div
                style={{
                  padding: '8px 12px',
                  borderRadius: 6,
                  background: 'var(--ant-color-fill-quaternary)',
                  border: '1px solid var(--ant-color-border-secondary)',
                  fontSize: 12,
                  lineHeight: 1.5,
                }}
              >
                <Text type="secondary" style={{ fontSize: 12, marginRight: 6 }}>
                  {isZh ? '活动简介 / 广告语：' : 'Activity Slogan:'}
                </Text>
                <Text style={{ fontSize: 12, whiteSpace: 'pre-wrap' }}>{generated.description.trim()}</Text>
              </div>
            )}
            <div className="rg-actions" style={{ marginBottom: 8 }}>
              <Button size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(generated.codes.join('\n'))}>
                {isZh ? '复制全部兑换码' : 'Copy all codes'}
              </Button>
              {generated.description?.trim() && (
                <Button
                  size="small"
                  icon={<CopyOutlined />}
                  onClick={() => copyToClipboard(`${generated.description!.trim()}\n${generated.codes.join('\n')}`)}
                >
                  {isZh ? '复制含广告语' : 'Copy with slogan'}
                </Button>
              )}
              <Button size="small" icon={<DownloadOutlined />} onClick={() => downloadCodes(generated.activity_id, generated.codes, generated.description)}>
                {isZh ? '下载全部' : 'Download all'}
              </Button>
              <Button size="small" onClick={() => { setGenerated(null); form.resetFields(); }}>
                {isZh ? '继续生成' : 'Generate more'}
              </Button>
              <Button size="small" type="primary" onClick={() => navigate(listUrl)}>
                {isZh ? '返回列表' : 'Back to list'}
              </Button>
            </div>
            <div className="rg-code-list">
              {generated.codes.map((code) => (
                <div key={code} className="rg-code-row">
                  <Text style={{ fontFamily: 'monospace', fontSize: 13 }}>{code}</Text>
                  <Space size={4}>
                    {generated.description?.trim() && (
                      <Button
                        type="text"
                        size="small"
                        onClick={() => copyToClipboard(`${generated.description!.trim()}\n${isZh ? '兑换码：' : 'Code: '}${code}`)}
                      >
                        {isZh ? '带广告语复制' : 'Copy w/ slogan'}
                      </Button>
                    )}
                    <Button type="text" size="small" icon={<CopyOutlined />} onClick={() => copyToClipboard(code)} />
                  </Space>
                </div>
              ))}
            </div>
          </div>
        </section>
      ) : (
        <Form
          form={form}
          layout="vertical"
          initialValues={formInitial}
          onFinish={handleCreate}
          className="rg-stack"
        >
          <section className="rg-section">
            <div className="rg-section-head">
              <div className="rg-section-title">{isZh ? '活动' : 'Activity'}</div>
            </div>
            <div className="rg-section-body">
              <Form.Item
                name="name"
                label={t('redemptions.name')}
                rules={[{ required: true, message: isZh ? '请填写活动名称' : 'Activity name is required' }]}
              >
                <Input placeholder={isZh ? '例如：新用户模型体验' : 'e.g. New user model trial'} />
              </Form.Item>
              <Form.Item
                name="description"
                label={isZh ? '活动简介' : 'Activity Slogan'}
                extra={isZh ? '可输入活动广告语或发放说明，在发放兑换及用户核销时展示。' : 'Optional promo slogan or description displayed when distributing and redeeming codes.'}
              >
                <Input.TextArea
                  rows={2}
                  maxLength={500}
                  showCount
                  placeholder={isZh ? '例如：限时福利！输入兑换码即享专属模型畅玩额度' : 'e.g. Limited-time offer! Redeem code for exclusive model quota'}
                />
              </Form.Item>
              <Form.Item
                name="remark"
                label={isZh ? '备注说明' : 'Admin Remark'}
                extra={isZh ? '仅后台管理员可见，可在兑换管理列表「活动ID」下方快速编辑。' : 'Visible to backend admins only. Can be quick-edited under Activity ID in the redemption list.'}
              >
                <Input
                  maxLength={500}
                  placeholder={isZh ? '例如：渠道合作A批次发放，内部跟进备注' : 'e.g. Partner batch A internal note'}
                />
              </Form.Item>
              <Form.Item
                name="code_type"
                label={isZh ? '兑换码类型' : 'Code type'}
                rules={[{ required: true }]}
                extra={
                  codeType === 'model_voucher'
                    ? (isZh ? '面额只在所选模型的 API 计费中扣除，不增加账户余额。' : 'Face value is spent only on the selected models and is not added to balance.')
                    : codeType === 'playground_space'
                      ? (isZh
                        ? '核销后增加创作中心 2026 的存储空间，不增加账户余额。等级未开放创作中心时，这段空间不会单独开通存储。'
                        : 'Adds Creative Center 2026 storage and does not credit the wallet. It does not open storage for a level that has no quota.')
                      : (isZh ? '核销后按所设面值分别计入用户系统余额与赠送金余额，可在全站消费。' : 'Credits the system wallet and/or gift wallet balance and can be spent site-wide.')
                }
              >
                <TwoChoiceButtons
                  options={[
                    { label: isZh ? '余额兑换码' : 'Balance code', value: 'balance' },
                    { label: isZh ? '模型卡券兑换码' : 'Model voucher', value: 'model_voucher' },
                    { label: isZh ? '创作空间兑换码' : 'Creative space', value: 'playground_space' },
                  ]}
                />
              </Form.Item>
            </div>
          </section>

          {codeType === 'model_voucher' && (
            <section className="rg-section">
              <div className="rg-section-head">
                <div className="rg-section-title">{isZh ? '可消耗模型与专属折扣' : 'Allowed models & discount'}</div>
                <div className="rg-page-desc">
                  {isZh
                    ? '额度只在勾选模型的 API 计费里扣除。可设置卡券统一折扣，或为下方已选固定模型单独配置专属折扣。'
                    : 'Spent only on checked models. Configure a unified coupon discount or per-model discount overrides.'}
                </div>
              </div>
              <div className="rg-section-body">
                <div
                  style={{
                    padding: '8px 10px',
                    marginBottom: 8,
                    borderRadius: 6,
                    background: 'var(--ant-color-fill-quaternary)',
                    border: '1px solid var(--ant-color-border-secondary)',
                  }}
                >
                  <Row gutter={12} align="middle">
                    <Col xs={24} sm={8}>
                      <Form.Item
                        name="voucher_discount"
                        label={isZh ? '卡券模型折扣倍率 (0.01~1)' : 'Coupon model discount (0.01~1)'}
                        extra={isZh ? '如 0.8 代表 8 折，1 为原价' : 'e.g. 0.8 = 20% off, 1 = original'}
                        style={{ marginBottom: 0 }}
                      >
                        <InputNumber
                          min={0.01}
                          max={1}
                          step={0.05}
                          precision={4}
                          style={{ width: '100%' }}
                          placeholder={isZh ? '默认 1 (不额外打折)' : 'Default 1'}
                        />
                      </Form.Item>
                    </Col>
                    <Col xs={24} sm={16}>
                      <div style={{ fontSize: 12, lineHeight: 1.5, color: 'var(--ant-color-text-secondary)' }}>
                        <div>
                          <Text strong style={{ fontSize: 12 }}>
                            {isZh ? '卡券折扣计费规则：' : 'Coupon discount rule: '}
                          </Text>
                          {isZh ? (
                            <>
                              有效期内消耗卡券面值时，优先按卡券设置的模型折扣价格计费（当前统一折扣：
                              <Text strong style={{ fontSize: 12, margin: '0 4px' }}>
                                {formatZhe(voucherDiscount, isZh)}
                              </Text>
                              ，下方已选固定模型可单独设置专属折扣）。
                            </>
                          ) : (
                            <>
                              Consuming voucher credit applies the coupon discount first (current unified:{' '}
                              <Text strong style={{ fontSize: 12 }}>
                                {formatZhe(voucherDiscount, isZh)}
                              </Text>
                              ; per-model overrides supported below).
                            </>
                          )}
                        </div>
                        <div>
                          {isZh
                            ? '若卡券折扣价高于模型折扣价则按模型价格计费，兜底始终不能低于模型的最低限价。'
                            : 'Falls back to model discount if lower, and is always clamped by the model floor price.'}
                        </div>
                      </div>
                    </Col>
                  </Row>
                </div>

                <Form.Item name="model_discounts" hidden>
                  <Input />
                </Form.Item>

                <Form.Item
                  name="allowed_models"
                  rules={[{
                    validator: async (_, value) => {
                      if (Array.isArray(value) && value.length > 0) return;
                      throw new Error(isZh ? '请选择至少一个模型' : 'Select at least one model');
                    },
                  }]}
                >
                  <ModelPicker
                    models={models}
                    providers={providers}
                    types={modelTypes}
                    loading={modelsLoading}
                    isZh={isZh}
                    voucherDiscount={voucherDiscount}
                    modelDiscounts={modelDiscounts}
                    onModelDiscountsChange={(next) => form.setFieldValue('model_discounts', next)}
                  />
                </Form.Item>
              </div>
            </section>
          )}

          <section className="rg-section">
            <div className="rg-section-head">
              <div className="rg-section-title">{isZh ? '面额与数量' : 'Value and quantity'}</div>
            </div>
            <div className="rg-section-body">
              {codeType === 'model_voucher' ? (
                <Row gutter={12}>
                  <Col xs={24} sm={8}>
                    <Form.Item
                      name="quota"
                      label={isZh ? `额度面值 (${currencySymbol})` : `${t('redemptions.quota')} (${currencySymbol})`}
                      rules={[
                        { required: true, message: isZh ? '请填写额度面值' : 'Amount is required' },
                        {
                          validator: async (_, value) => {
                            if (!(Number(value) > 0)) {
                              throw new Error(isZh ? '额度面值必须大于 0' : 'Amount must be greater than 0');
                            }
                          },
                        },
                      ]}
                      extra={isZh ? '实际可消费抵扣的模型卡券金额' : 'Actual consumable voucher credit'}
                      style={{ marginBottom: 0 }}
                    >
                      <InputNumber min={0.01} style={{ width: '100%' }} />
                    </Form.Item>
                  </Col>
                  <Col xs={24} sm={8}>
                    <Form.Item
                      name="worth_quota"
                      label={isZh ? `额度面额价值 (${currencySymbol})` : `Voucher Worth (${currencySymbol})`}
                      extra={isZh ? '告知用户该卡券价值多少钱（选填，填 0 或留空不单独展示）' : 'Display worth shown to users (optional)'}
                      style={{ marginBottom: 0 }}
                    >
                      <InputNumber min={0} style={{ width: '100%' }} placeholder={isZh ? '例如: 200 (选填)' : 'Optional'} />
                    </Form.Item>
                  </Col>
                  <Col xs={24} sm={8}>
                    <Form.Item name="count" label={isZh ? '生成数量' : t('common.count')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                      <InputNumber min={1} max={100} style={{ width: '100%' }} />
                    </Form.Item>
                  </Col>
                </Row>
              ) : codeType === 'playground_space' ? (
                <Row gutter={12}>
                  <Col span={12}>
                    <Form.Item
                      name="quota"
                      label={isZh ? '存储空间 (MB)' : 'Storage (MB)'}
                      rules={[
                        { required: true, message: isZh ? '请填写额度' : 'Amount is required' },
                        {
                          validator: async (_, value) => {
                            const num = Number(value);
                            if (!Number.isInteger(num) || num < 1 || num > 1048576) {
                              throw new Error(isZh ? '请填写 1 到 1048576 的整数 MB' : 'Enter a whole number of MB from 1 to 1048576');
                            }
                          },
                        },
                      ]}
                      extra={isZh ? '整数 MB，兑换后加到该用户现有创作中心配额上。多张码累加。' : 'Whole megabytes. Added on top of the user quota. Multiple codes stack.'}
                      style={{ marginBottom: 0 }}
                    >
                      <InputNumber
                        min={1}
                        max={1048576}
                        precision={0}
                        style={{ width: '100%' }}
                      />
                    </Form.Item>
                  </Col>
                  <Col span={12}>
                    <Form.Item name="count" label={isZh ? '生成数量' : t('common.count')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                      <InputNumber min={1} max={100} style={{ width: '100%' }} />
                    </Form.Item>
                  </Col>
                </Row>
              ) : (
                <Row gutter={12}>
                  <Col xs={24} sm={8}>
                    <Form.Item
                      name="quota"
                      dependencies={['gift_quota']}
                      label={isZh ? `系统余额面值 (${currencySymbol})` : `System Balance (${currencySymbol})`}
                      rules={[
                        {
                          validator: async (_, value) => {
                            const sysVal = Number(value || 0);
                            const giftVal = Number(form.getFieldValue('gift_quota') || 0);
                            if (sysVal < 0) {
                              throw new Error(isZh ? '系统余额面值不能小于 0' : 'Cannot be negative');
                            }
                            if (sysVal <= 0 && giftVal <= 0) {
                              throw new Error(isZh ? '系统余额与赠送金面值至少有一项大于 0' : 'Enter system or gift balance (> 0)');
                            }
                          },
                        },
                      ]}
                      extra={isZh ? '兑换后计入用户系统钱包余额，不发放可填 0' : 'Credited to system wallet (0 for none)'}
                      style={{ marginBottom: 0 }}
                    >
                      <InputNumber min={0} step={1} style={{ width: '100%' }} placeholder="0" />
                    </Form.Item>
                  </Col>
                  <Col xs={24} sm={8}>
                    <Form.Item
                      name="gift_quota"
                      dependencies={['quota']}
                      label={isZh ? `赠送金余额面值 (${currencySymbol})` : `Gift Balance (${currencySymbol})`}
                      rules={[
                        {
                          validator: async (_, value) => {
                            const giftVal = Number(value || 0);
                            const sysVal = Number(form.getFieldValue('quota') || 0);
                            if (giftVal < 0) {
                              throw new Error(isZh ? '赠送金余额面值不能小于 0' : 'Cannot be negative');
                            }
                            if (sysVal <= 0 && giftVal <= 0) {
                              throw new Error(isZh ? '系统余额与赠送金面值至少有一项大于 0' : 'Enter system or gift balance (> 0)');
                            }
                          },
                        },
                      ]}
                      extra={isZh ? '兑换后计入用户赠送钱包余额，不发放可填 0' : 'Credited to gift wallet (0 for none)'}
                      style={{ marginBottom: 0 }}
                    >
                      <InputNumber min={0} step={1} style={{ width: '100%' }} placeholder="0" />
                    </Form.Item>
                  </Col>
                  <Col xs={24} sm={8}>
                    <Form.Item name="count" label={isZh ? '生成数量' : t('common.count')} rules={[{ required: true }]} style={{ marginBottom: 0 }}>
                      <InputNumber min={1} max={100} style={{ width: '100%' }} />
                    </Form.Item>
                  </Col>
                </Row>
              )}
            </div>
          </section>

          <section className="rg-section">
            <div className="rg-section-head">
              <div className="rg-section-title">{isZh ? '生效与到期' : 'Schedule'}</div>
            </div>
            <div className="rg-section-body">
              <Form.Item name="effective_immediately" label={isZh ? '开始生效' : 'Starts'} rules={[{ required: true }]}>
                <TwoChoiceButtons
                  options={[
                    { label: isZh ? '生成后立即生效' : 'Effective immediately', value: true },
                    { label: isZh ? '指定生效时间' : 'Set start time', value: false },
                  ]}
                />
              </Form.Item>
              {effectiveImmediately === false && (
                <Form.Item
                  name="starts_at"
                  label={isZh ? '生效时间' : 'Effective from'}
                  rules={[{ required: true, message: isZh ? '请设置开始生效时间' : 'Please select a start time' }]}
                  extra={isZh ? '可直接输入天数/小时或点击快捷按钮推算，也可在日期框中自选' : 'Input days/hours or pick shortcut to calculate, or select directly in calendar'}
                >
                  <QuickTimeInput
                    baseTime={dayjs()}
                    isZh={isZh}
                    presets={commonTimePresets}
                    durationLabel={isZh ? '快速输入：' : 'Quick input:'}
                    disabledDate={(current) => !!current && current < dayjs().startOf('day')}
                    placeholder={isZh ? '选择具体生效时间或由上方时长自动计算' : 'Pick start time or auto-calculate above'}
                  />
                </Form.Item>
              )}
              <Form.Item
                name="permanent"
                label={isZh ? '兑换码有效期' : 'Code validity'}
                extra={codeType === 'model_voucher'
                  ? (isZh ? '只决定这张码到什么时候还能输入兑换。已经兑出去的模型卡券不看这个时间。' : 'Only limits when this code can still be redeemed. A voucher already granted ignores this time.')
                  : codeType === 'playground_space'
                    ? (isZh ? '只决定这张码到什么时候还能输入兑换。已经兑出去的存储空间不看这个时间。' : 'Only limits when this code can still be redeemed. Storage already granted ignores this time.')
                    : undefined}
                rules={[{ required: true }]}
              >
                <TwoChoiceButtons
                  options={[
                    { label: isZh ? '长期有效' : 'Permanent', value: true },
                    { label: isZh ? '设置到期时间' : 'Set expiry time', value: false },
                  ]}
                />
              </Form.Item>
              {permanent === false && (
                <Form.Item
                  name="expires_at"
                  label={isZh ? '到期时间' : 'Expires at'}
                  rules={[{ required: true, message: isZh ? '请设置到期时间' : 'Please select expiry time' }]}
                  extra={isZh ? '可直接输入有效天数/小时或点击快捷按钮推算，也可在日期框中自选' : 'Input valid days/hours or pick shortcut to calculate, or select directly in calendar'}
                >
                  <QuickTimeInput
                    baseTime={baseForExpiry}
                    isZh={isZh}
                    presets={commonTimePresets}
                    durationLabel={isZh ? '快速输入：' : 'Quick input:'}
                    disabledDate={(current) => !!current && current < baseForExpiry.startOf('day')}
                    placeholder={isZh ? '选择具体到期时间或由上方时长自动计算' : 'Pick expiry time or auto-calculate above'}
                  />
                </Form.Item>
              )}
            </div>
          </section>

          {(codeType === 'model_voucher' || codeType === 'playground_space') && (
            <section className="rg-section">
              <div className="rg-section-head">
                <div className="rg-section-title">
                  {codeType === 'playground_space'
                    ? (isZh ? '创作空间兑换后使用有效期' : 'Storage validity after redeem')
                    : (isZh ? '模型卡券兑换后使用有效期' : 'Voucher validity after redeem')}
                </div>
              </div>
              <div className="rg-section-body">
                <Form.Item
                  name="grant_permanent"
                  label={codeType === 'playground_space'
                    ? (isZh ? '创作空间兑换后使用有效期' : 'Storage validity after redeem')
                    : (isZh ? '模型卡券兑换后使用有效期' : 'Voucher validity after redeem')}
                  extra={codeType === 'playground_space'
                    ? (isZh ? '从兑换成功那天起算。长期保留一直有效。设置时长后，到期不能继续占用超出的空间，已上传的文件不删除。' : 'Counted from the successful redeem. Keep permanently, or set a length. Uploaded files stay after it ends.')
                    : (isZh ? '从兑换成功那天起算。长期保留表示卡券一直可用，也可以自己选使用时长。' : 'Counted from the successful redeem. Keep the voucher permanently, or set how long it can be used.')}
                  rules={[{ required: true }]}
                >
                  <TwoChoiceButtons
                    options={[
                      { label: isZh ? '长期保留' : 'Keep permanently', value: true },
                      { label: isZh ? '设置时长' : 'Set a length', value: false },
                    ]}
                  />
                </Form.Item>
                {grantPermanent === false && (
                  <Form.Item
                    name="valid_days"
                    label={isZh ? '使用时长' : 'Length'}
                    rules={[{ required: true, message: isZh ? '请选择使用时长' : 'Choose a length' }]}
                  >
                    <GrantDurationInput isZh={isZh} />
                  </Form.Item>
                )}
              </div>
            </section>
          )}

          <section className="rg-section">
            <div className="rg-section-head">
              <div className="rg-section-title">{isZh ? '兑换规则' : 'Redemption rules'}</div>
            </div>
            <div className="rg-section-body">
              <Form.Item
                name="max_uses_limit"
                label={isZh ? '单兑换码兑换次数' : 'Uses per code'}
                extra={isZh ? '每个兑换码各自可被兑换的次数。点「不限制」则不设上限。' : 'How many times each code can be redeemed. Choose Unlimited for no cap.'}
              >
                <UsageLimitInput isZh={isZh} max={100000} />
              </Form.Item>
              <Form.Item
                name="activity_limit"
                label={isZh ? '单用户可参与次数' : 'Max redemptions per user'}
                extra={isZh ? '同一用户兑换本活动任意兑换码都计入。点「不限制」则不设上限。' : 'Counts across every code in this activity. Choose Unlimited for no cap.'}
              >
                <UsageLimitInput isZh={isZh} max={10000} />
              </Form.Item>
            </div>
          </section>

          <section className="rg-section">
            <div className="rg-section-head">
              <div className="rg-section-title">{isZh ? '兑换条件' : 'Eligibility'}</div>
              <div className="rg-page-desc">
                {isZh
                  ? '不设置则任何人可兑。账户余额只看系统余额。消费是已结算调用费用。充值只算在线支付和管理员充入系统余额，不含兑换码和赠送。当月按站点时区。'
                  : 'Leave this off to allow anyone. Balance is the system wallet. Spend is settled usage. Recharge is online payment and admin system-wallet credit. The month uses the site timezone.'}
              </div>
            </div>
            <div className="rg-section-body">
              <Form.Item noStyle shouldUpdate>
                {() => {
                  const enabled = !!form.getFieldValue('eligibility_enabled');
                  const rows: Array<{ on: string; amount: string; label: string }> = [
                    { on: 'cond_min_balance_on', amount: 'cond_min_balance', label: isZh ? '账户余额' : 'Account balance' },
                    { on: 'cond_month_spend_on', amount: 'cond_month_spend', label: isZh ? '当月消费' : 'Spend this month' },
                    { on: 'cond_total_spend_on', amount: 'cond_total_spend', label: isZh ? '总消费' : 'Total spend' },
                    { on: 'cond_month_recharge_on', amount: 'cond_month_recharge', label: isZh ? '当月充值' : 'Recharge this month' },
                    { on: 'cond_total_recharge_on', amount: 'cond_total_recharge', label: isZh ? '总充值' : 'Total recharge' },
                  ];
                  return (
                    <>
                      <Form.Item name="eligibility_enabled" label={isZh ? '是否限制' : 'Restriction'} rules={[{ required: true }]}>
                        <TwoChoiceButtons
                          options={[
                            { label: isZh ? '不限制' : 'No restriction', value: false },
                            { label: isZh ? '设置条件' : 'Set conditions', value: true },
                          ]}
                        />
                      </Form.Item>
                      {enabled && (
                        <>
                          <Form.Item name="condition_match" label={isZh ? '成立方式' : 'How they combine'} rules={[{ required: true }]}>
                            <TwoChoiceButtons
                              options={[
                                { label: isZh ? '全部满足' : 'All required', value: 0 },
                                { label: isZh ? '满足其中一项' : 'Any one', value: 1 },
                              ]}
                            />
                          </Form.Item>
                          {rows.map((row) => (
                            <Form.Item key={row.on} label={row.label}>
                              <GateAmount
                                enabled={!!form.getFieldValue(row.on)}
                                onToggle={(on) => form.setFieldValue(row.on, on)}
                                amount={form.getFieldValue(row.amount)}
                                onAmount={(amount) => form.setFieldValue(row.amount, amount)}
                                isZh={isZh}
                                currencySymbol={currencySymbol}
                              />
                            </Form.Item>
                          ))}
                        </>
                      )}
                    </>
                  );
                }}
              </Form.Item>
            </div>
          </section>

          <div className="rg-footer">
            <Button size="small" onClick={() => navigate(listUrl)}>{t('common.cancel')}</Button>
            <Button size="small" type="primary" loading={submitting} onClick={() => form.submit()}>
              {t('redemptions.add')}
            </Button>
          </div>
        </Form>
      )}
      <style>{`
        .redemption-generate .rg-page-head {
          display: flex;
          align-items: flex-start;
          gap: 8px;
          margin-bottom: 10px;
        }
        .redemption-generate .rg-page-title {
          font-size: 16px;
          font-weight: 600;
          line-height: 22px;
        }
        .redemption-generate .rg-page-desc {
          margin-top: 2px;
          font-size: 12px;
          line-height: 1.4;
          color: var(--ant-color-text-secondary);
        }
        .redemption-generate .rg-stack {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .redemption-generate .rg-stack-tight {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }
        .redemption-generate .rg-section {
          border: 1px solid var(--ant-color-border-secondary);
          border-radius: 8px;
          background: var(--ant-color-bg-container);
        }
        .redemption-generate .rg-section-head {
          padding: 8px 12px 0;
        }
        .redemption-generate .rg-section-title {
          font-size: 13px;
          font-weight: 600;
          line-height: 18px;
        }
        .redemption-generate .rg-section-body {
          padding: 8px 12px 10px;
        }
        .redemption-generate .rg-label {
          margin-bottom: 6px;
          font-size: 13px;
          line-height: 18px;
        }
        .redemption-generate .ant-form-item {
          margin-bottom: 8px;
        }
        .redemption-generate .rg-section-body > .ant-form-item:last-child {
          margin-bottom: 0;
        }
        .redemption-generate .ant-form-item .ant-form-item-label {
          padding-bottom: 2px;
        }
        .redemption-generate .ant-form-item .ant-form-item-label > label {
          font-size: 13px;
          height: auto;
        }
        .redemption-generate .ant-form-item .ant-form-item-extra {
          margin-top: 2px;
          min-height: 0;
          font-size: 12px;
        }
        .redemption-generate .ant-input,
        .redemption-generate .ant-input-number,
        .redemption-generate .ant-picker {
          width: 100%;
          border-radius: 6px;
        }
        .redemption-generate .ant-input:focus,
        .redemption-generate .ant-input-focused,
        .redemption-generate .ant-input-number-focused,
        .redemption-generate .ant-picker-focused {
          box-shadow: 0 0 0 2px var(--ant-color-fill-secondary);
        }
        .redemption-generate .rg-footer {
          position: sticky;
          bottom: 0;
          z-index: 2;
          display: flex;
          justify-content: flex-end;
          gap: 8px;
          padding: 8px 0 0;
          background: var(--ant-color-bg-container);
        }
        .redemption-generate .rg-actions {
          display: flex;
          flex-wrap: wrap;
          gap: 6px;
        }
        .redemption-generate .rg-usage {
          display: inline-flex;
          align-items: stretch;
          max-width: 100%;
          border: 1px solid var(--ant-color-border-secondary);
          border-radius: 6px;
          overflow: hidden;
        }
        .redemption-generate .rg-usage-count {
          position: relative;
          display: flex;
          align-items: center;
          gap: 2px;
          min-width: 0;
          padding-right: 8px;
        }
        .redemption-generate .rg-usage.is-count .rg-usage-count {
          background: var(--ant-color-fill-quaternary);
        }
        .redemption-generate .rg-usage.is-unlimited .rg-usage-count {
          opacity: 0.4;
        }
        .redemption-generate .rg-usage .ant-input-number {
          width: 96px;
          background: transparent;
          box-shadow: none;
        }
        .redemption-generate .rg-usage-mask {
          position: absolute;
          inset: 0;
          border: 0;
          padding: 0;
          background: transparent;
          cursor: pointer;
        }
        .redemption-generate .rg-usage .ant-input-number-focused,
        .redemption-generate .rg-usage .ant-input-number:focus-within {
          box-shadow: none;
        }
        .redemption-generate .rg-usage-unit {
          font-size: 13px;
          line-height: 32px;
          white-space: nowrap;
        }
        .redemption-generate .rg-usage-unlimit {
          border: 0;
          border-left: 1px solid var(--ant-color-border-secondary);
          background: transparent;
          padding: 0 14px;
          font: inherit;
          font-size: 13px;
          line-height: 32px;
          color: inherit;
          cursor: pointer;
        }
        .redemption-generate .rg-usage.is-unlimited .rg-usage-unlimit {
          background: var(--ant-color-fill-quaternary);
          font-weight: 600;
        }
        .redemption-generate .rg-code-list {
          border: 1px solid var(--ant-color-border-secondary);
          border-radius: 6px;
          overflow: hidden;
        }
        .redemption-generate .rg-code-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 8px;
          padding: 2px 8px 2px 10px;
          border-bottom: 1px solid var(--ant-color-border-secondary);
        }
        .redemption-generate .rg-code-row:last-child {
          border-bottom: 0;
        }
        .redemption-model-filter > div {
          margin-bottom: 6px !important;
          padding: 6px 8px !important;
          border-radius: 6px !important;
        }
        .redemption-model-filter > div > div {
          align-items: flex-start !important;
        }
        .redemption-model-filter > div > div:not(:last-child) {
          margin-bottom: 4px !important;
        }
        .redemption-model-filter > div > div > .ant-typography {
          width: 72px !important;
          flex-shrink: 0;
          font-size: 12px !important;
          line-height: 22px !important;
          padding-top: 0 !important;
          white-space: nowrap !important;
        }
        .redemption-model-filter > div > div > div {
          gap: 4px 6px !important;
        }
        .redemption-model-filter > div > div > div > div {
          width: auto !important;
          height: auto !important;
          max-width: none !important;
          padding: 1px 8px !important;
          border-radius: 12px !important;
          font-size: 12px !important;
          line-height: 20px !important;
          gap: 4px !important;
        }
        .redemption-model-filter > div > div > div > div > div {
          width: auto !important;
          height: auto !important;
          max-width: none !important;
          flex-shrink: 0;
        }
        .redemption-model-filter > div > div > div > div > div:last-child {
          flex-shrink: 1;
          width: auto !important;
          height: auto !important;
        }
        .redemption-model-filter > div > div > div > div img,
        .redemption-model-filter > div > div > div > div svg {
          width: 14px !important;
          height: 14px !important;
          flex-shrink: 0;
        }
        .redemption-model-filter > div > div > div > div span {
          font-size: 11px !important;
        }
        .redemption-model-scroll {
          max-height: 320px;
          overflow: auto;
        }
        .redemption-model-scroll .ant-table-thead > tr > th {
          position: sticky;
          top: 0;
          z-index: 2;
          background: var(--ant-color-bg-container);
        }
        .redemption-model-table .ant-table-cell {
          padding: 3px 8px !important;
          font-size: 12px;
          line-height: 16px;
        }
        .redemption-model-table .rg-model-full,
        .rg-model-full {
          display: block;
          font-size: 12px;
          line-height: 16px;
          white-space: normal;
          overflow-wrap: anywhere;
          word-break: break-word;
        }
        .redemption-model-table .ant-table-cell:last-child {
          width: 96px;
          max-width: 96px;
        }
        .redemption-model-table .ant-table-pagination {
          margin: 8px 0 0 !important;
        }
      `}</style>
    </Card>
  );
};

export default RedemptionGenerate;
