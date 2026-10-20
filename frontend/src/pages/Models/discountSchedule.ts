import dayjs from 'dayjs';
import {
  ALL_WEEKDAYS,
  expandTimeMultiplierIntervals,
  findOverlappingAllDayDays,
  findOverlappingTimeMultipliers,
  isAllDayMultiplier,
  normalizeWeekdays,
  type TimeMultiplierRule,
} from '../../utils/timeMultipliers';

export type DiscountSlotForm = {
  start?: unknown;
  end?: unknown;
  days?: number[];
  all_day?: boolean;
  site_discount?: number;
  global_discount?: number;
};

function clockString(v: unknown): string {
  if (v && typeof v === 'object' && typeof (v as { format?: unknown }).format === 'function') {
    return (v as { format: (pattern: string) => string }).format('HH:mm');
  }
  const matched = /^(\d{1,2}):(\d{2})/.exec(String(v ?? ''));
  if (!matched) return '';
  return `${matched[1].padStart(2, '0')}:${matched[2]}`;
}

function clockMinutes(v: unknown): number | null {
  if (v && typeof v === 'object' && typeof (v as { hour?: unknown }).hour === 'function') {
    const clock = v as { hour: () => number; minute: () => number };
    return clock.hour() * 60 + clock.minute();
  }
  const text = clockString(v);
  const matched = /^(\d{2}):(\d{2})$/.exec(text);
  if (!matched) return null;
  return Number(matched[1]) * 60 + Number(matched[2]);
}

export function unpackDiscountSchedule(raw?: string | null): {
  enabled: boolean;
  slots: DiscountSlotForm[];
  edit_tz: 'operation' | 'admin_display';
} {
  let enabled = false;
  let slots: DiscountSlotForm[] = [];
  let edit_tz: 'operation' | 'admin_display' = 'operation';
  try {
    if (raw) {
      const parsed = JSON.parse(raw);
      enabled = !!parsed?.enabled;
      if (Array.isArray(parsed?.slots)) slots = parsed.slots;
      if (parsed?.edit_tz === 'admin_display') edit_tz = 'admin_display';
    }
  } catch {
    enabled = false;
    slots = [];
  }
  return {
    enabled,
    edit_tz,
    slots: slots
      .map((item) => ({
        all_day: !!item.all_day,
        start: item.start ? dayjs(String(item.start), 'HH:mm') : null,
        end: item.end ? dayjs(String(item.end), 'HH:mm') : null,
        days: normalizeWeekdays(item.days),
        site_discount: Number(item.site_discount ?? 1),
        global_discount: Number(item.global_discount ?? 1),
      }))
      .filter((item) => item.all_day || (item.start && item.end)),
  };
}

export function discountScheduleOn(raw?: string | null): boolean {
  const parsed = unpackDiscountSchedule(raw);
  return parsed.enabled && parsed.slots.length > 0;
}

export function packDiscountSchedule(
  enabled: boolean,
  slots: DiscountSlotForm[] | undefined,
  editTz: 'operation' | 'admin_display' = 'operation',
): string {
  return JSON.stringify({
    enabled: !!enabled,
    edit_tz: editTz === 'admin_display' ? 'admin_display' : 'operation',
    slots: (slots || []).map((item) => ({
      all_day: !!item.all_day,
      start: item.all_day ? '' : clockString(item.start),
      end: item.all_day ? '' : clockString(item.end),
      days: normalizeWeekdays(item.days),
      site_discount: Number(item.site_discount ?? 1),
      global_discount: Number(item.global_discount ?? 1),
    })),
  });
}

export function validateDiscountScheduleForm(
  enabled: boolean,
  slots: DiscountSlotForm[] | undefined,
  siteOn: boolean,
  globalOn: boolean,
): string | null {
  if (!enabled) return null;
  if (!globalOn) return '开启全站折扣后才能规划时段折扣';
  const list = slots || [];
  if (!list.length) return '已开启时段折扣，请至少添加一条时段或全天规则';
  const intervals = [];
  for (let i = 0; i < list.length; i += 1) {
    const item = list[i];
    const site = item.site_discount == null || (item.site_discount as unknown) === '' ? NaN : Number(item.site_discount);
    const global = item.global_discount == null || (item.global_discount as unknown) === '' ? NaN : Number(item.global_discount);
    if (siteOn && !(Number.isFinite(site) && site >= 0)) return `第 ${i + 1} 条规则的折扣限价必须大于等于 0（0 为免费）`;
    if (globalOn && !(Number.isFinite(global) && global >= 0)) return `第 ${i + 1} 条规则的全站折扣必须大于等于 0（0 为免费）`;
    if (siteOn && globalOn && global < site) {
      const shown = (n: number) => (n === 0 ? '免费' : String(n));
      return `第 ${i + 1} 条规则的全站折扣 ${shown(global)} 低于折扣限价 ${shown(site)}`;
    }
    if (isAllDayMultiplier(item)) continue;
    const startMin = clockMinutes(item.start);
    const endMin = clockMinutes(item.end);
    if (startMin == null || endMin == null) return '请完整填写所有时间段的起止时间';
    if (startMin === endMin) return `第 ${i + 1} 个时间段的起止时间不能相同`;
    intervals.push(...expandTimeMultiplierIntervals(startMin, endMin, normalizeWeekdays(item.days), i));
  }
  const overlap = findOverlappingTimeMultipliers(intervals);
  if (overlap) {
    const [left, right] = overlap;
    return `配置的时间段存在重叠（第 ${left.index + 1} 组与第 ${right.index + 1} 组）`;
  }
  const allDayOverlap = findOverlappingAllDayDays(list);
  if (allDayOverlap) {
    return `全天规则重复（第 ${allDayOverlap.a + 1} 组与第 ${allDayOverlap.b + 1} 组）`;
  }
  return null;
}

export function discountChartItems(
  slots: DiscountSlotForm[] | undefined,
  pick: 'site_discount' | 'global_discount',
  fallback: number,
): TimeMultiplierRule[] {
  const items: TimeMultiplierRule[] = (slots || []).map((item) => {
    const raw = item[pick];
    const num = raw == null || (raw as unknown) === '' ? fallback : Number(raw);
    return {
      start: item.start,
      end: item.end,
      days: normalizeWeekdays(item.days),
      all_day: !!item.all_day,
      multiplier: Number.isFinite(num) && num >= 0 ? num : fallback,
    };
  });
  items.push({ all_day: true, days: [...ALL_WEEKDAYS], multiplier: fallback });
  return items;
}
