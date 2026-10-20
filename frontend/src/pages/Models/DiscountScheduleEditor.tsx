import React, { useEffect, useState } from 'react';
import { Button, Col, Form, InputNumber, Radio, Row, Switch, TimePicker, Typography } from 'antd';
import { DeleteOutlined, PlusOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import utc from 'dayjs/plugin/utc';
import timezone from 'dayjs/plugin/timezone';
import { formatWallClock } from '../../utils/timedisplay';
import TimeMultiplierChart from './TimeMultiplierChart';
import { convertScheduleSlots, scheduleInputTzName, type ScheduleTzMode } from './scheduleTzConvert';
import { discountChartItems, type DiscountSlotForm } from './discountSchedule';
import {
  ALL_WEEKDAYS,
  formatWeekdayRange,
  isAllDayMultiplier,
  isAllWeekdays,
  isFreeTimeMultiplier,
  normalizeWeekdays,
  resolveTimeMultiplierAt,
} from '../../utils/timeMultipliers';

dayjs.extend(utc);
dayjs.extend(timezone);

const { Text } = Typography;

const WEEKDAY_CHIPS = [
  { v: 1, l: '一' },
  { v: 2, l: '二' },
  { v: 3, l: '三' },
  { v: 4, l: '四' },
  { v: 5, l: '五' },
  { v: 6, l: '六' },
  { v: 7, l: '日' },
];

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
          <Button
            key={chip.v}
            size="small"
            type={checked ? 'primary' : 'default'}
            onClick={() => toggle(chip.v)}
            style={{ minWidth: 28, padding: '0 6px' }}
          >
            {chip.l}
          </Button>
        );
      })}
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([...ALL_WEEKDAYS])}>每天</Button>
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([1, 2, 3, 4, 5])}>工作日</Button>
      <Button type="link" size="small" style={{ padding: '0 4px' }} onClick={() => onChange?.([6, 7])}>周末</Button>
    </div>
  );
};

function currentClock(defaultTz: string) {
  const now = dayjs();
  try {
    if (defaultTz.startsWith('UTC') || /^[+-]\d/.test(defaultTz)) {
      return now.utcOffset(defaultTz.replace('UTC', ''));
    }
    if (defaultTz) return now.tz(defaultTz);
  } catch {
    return now;
  }
  return now;
}

function discountRateText(rate: number): string {
  return isFreeTimeMultiplier(rate) ? '免费' : rate.toFixed(2);
}

const DiscountRateInput: React.FC<{
  value?: number | null;
  onChange?: (value: number | null) => void;
  addonBefore?: React.ReactNode;
}> = ({ value, onChange, addonBefore }) => {
  const [editing, setEditing] = useState(false);
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
      min={0}
      precision={2}
      step={0.1}
      style={{ width: '100%' }}
      addonBefore={addonBefore}
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

const DiscountScheduleEditor: React.FC<{
  isLight: boolean;
  operationTz: string;
  adminTz: string;
}> = ({ isLight, operationTz, adminTz }) => {
  const form = Form.useFormInstance();
  const enabled = Form.useWatch('discount_schedule_enabled', form);
  const siteOn = Form.useWatch('site_discount_enabled', form);
  const globalOn = Form.useWatch('global_discount_enabled', form);
  const inputTzMode = (Form.useWatch('discount_schedule_input_tz', form) || 'operation') as ScheduleTzMode;
  const siteFallback = Number(Form.useWatch('site_discount', form) ?? 1);
  const globalFallback = Number(Form.useWatch('global_discount', form) ?? 1);
  const slots = (Form.useWatch('discount_slots', form) || []) as DiscountSlotForm[];
  const clockTz = scheduleInputTzName(inputTzMode, operationTz, adminTz);
  const [now, setNow] = useState(() => currentClock(clockTz));

  useEffect(() => {
    const timer = setInterval(() => setNow(currentClock(clockTz)), 1000);
    return () => clearInterval(timer);
  }, [clockTz]);

  const weekday = now.day() === 0 ? 7 : now.day();
  const minutes = now.hour() * 60 + now.minute();
  const siteNow = enabled && siteOn
    ? resolveTimeMultiplierAt(discountChartItems(slots, 'site_discount', siteFallback), false, weekday, minutes)
    : siteFallback;
  const globalNow = enabled && globalOn
    ? resolveTimeMultiplierAt(discountChartItems(slots, 'global_discount', globalFallback), false, weekday, minutes)
    : globalFallback;

  return (
    <div style={{
      display: globalOn ? 'block' : 'none',
      marginBottom: 16,
      padding: 12,
      borderRadius: 8,
      border: isLight ? '1px solid #f0f0f0' : '1px solid rgba(255,255,255,0.08)',
      background: isLight ? 'rgba(22,119,255,0.03)' : 'rgba(22,119,255,0.06)',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 8 }}>
        <div>
          <Text strong>高级设置 · 按天规划时段折扣</Text>
          <Text type="secondary" style={{ fontSize: 12, display: 'block', marginTop: 4 }}>
            {inputTzMode === 'admin_display' ? '后台时间' : '运营时间'}: {formatWallClock(clockTz)} · {clockTz}
            {enabled && siteOn ? ` · 此刻折扣限价 ${discountRateText(siteNow)}` : ''}
            {enabled && globalOn ? ` · 全站折扣 ${discountRateText(globalNow)}` : ''}
          </Text>
        </div>
        <Form.Item name="discount_schedule_enabled" valuePropName="checked" initialValue={false} style={{ margin: 0 }}>
          <Switch size="small" checkedChildren="开启" unCheckedChildren="关闭" />
        </Form.Item>
      </div>
      <Form.Item name="discount_schedule_input_tz" initialValue="operation" style={{ marginBottom: 8 }}>
        <Radio.Group
          buttonStyle="solid"
          size="small"
          onChange={(e) => {
            const next = e.target.value as ScheduleTzMode;
            const fromTz = scheduleInputTzName(inputTzMode, operationTz, adminTz);
            const toTz = scheduleInputTzName(next, operationTz, adminTz);
            form.setFieldsValue({
              discount_slots: convertScheduleSlots(slots, fromTz, toTz),
            });
          }}
        >
          <Radio.Button value="operation">运营时区</Radio.Button>
          <Radio.Button value="admin_display">后台时区</Radio.Button>
        </Radio.Group>
      </Form.Item>
      <Text type="secondary" style={{ fontSize: 12, display: 'block' }}>
        按所选时钟填时段，保存后按运营时区计费。未命中用上方统一折扣。填 0 为免费；全站为免费时，限价也要是免费。
      </Text>
      <div style={{ display: enabled ? 'block' : 'none', marginTop: 12 }}>
        <Form.List name="discount_slots" initialValue={[]}>
          {(fields, { add, remove }) => (
            <>
              {fields.map(({ key, name: listName, ...restField }) => {
                const row = slots[listName] || {};
                const allDay = isAllDayMultiplier(row);
                const daysLabel = isAllWeekdays(row.days)
                  ? '每天'
                  : formatWeekdayRange(row.days, WEEKDAY_CHIPS.map((chip) => chip.l), '每天');
                return (
                  <div
                    key={key}
                    style={{
                      marginBottom: 12,
                      padding: '12px 12px 8px',
                      borderRadius: 8,
                      border: isLight ? '1px solid #f0f0f0' : '1px solid #303030',
                      background: isLight ? '#fff' : 'rgba(0,0,0,0.15)',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 8 }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <Text style={{ fontSize: 13 }}>{allDay ? '全天' : '时段'}</Text>
                        <Form.Item {...restField} name={[listName, 'all_day']} valuePropName="checked" style={{ margin: 0 }}>
                          <Switch size="small" checkedChildren="全天" unCheckedChildren="时段" />
                        </Form.Item>
                      </div>
                      <Button type="text" danger icon={<DeleteOutlined />} onClick={() => remove(listName)} size="small" />
                    </div>
                    {!allDay && (
                      <Row gutter={12}>
                        <Col xs={24} sm={12}>
                          <Form.Item
                            {...restField}
                            name={[listName, 'start']}
                            rules={globalOn && enabled && !allDay ? [{ required: true, message: '选择开始时间' }] : []}
                            style={{ marginBottom: 8 }}
                          >
                            <TimePicker placeholder="开始时间" format="HH:mm" style={{ width: '100%' }} allowClear={false} needConfirm={false} />
                          </Form.Item>
                        </Col>
                        <Col xs={24} sm={12}>
                          <Form.Item
                            {...restField}
                            name={[listName, 'end']}
                            rules={globalOn && enabled && !allDay ? [{ required: true, message: '选择结束时间' }] : []}
                            style={{ marginBottom: 8 }}
                          >
                            <TimePicker placeholder="结束时间" format="HH:mm" style={{ width: '100%' }} allowClear={false} needConfirm={false} />
                          </Form.Item>
                        </Col>
                      </Row>
                    )}
                    <Row gutter={12}>
                      <Col xs={24} sm={12} style={{ display: siteOn ? 'block' : 'none' }}>
                        <Form.Item
                          {...restField}
                          name={[listName, 'site_discount']}
                          style={{ marginBottom: 8 }}
                          initialValue={1}
                        >
                          <DiscountRateInput addonBefore="限价" />
                        </Form.Item>
                      </Col>
                      <Col xs={24} sm={12} style={{ display: globalOn ? 'block' : 'none' }}>
                        <Form.Item
                          {...restField}
                          name={[listName, 'global_discount']}
                          style={{ marginBottom: 8 }}
                          initialValue={1}
                        >
                          <DiscountRateInput addonBefore="全站" />
                        </Form.Item>
                      </Col>
                    </Row>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap', marginBottom: 4 }}>
                      <Text type="secondary" style={{ fontSize: 12 }}>星期</Text>
                      <Form.Item {...restField} name={[listName, 'days']} initialValue={[...ALL_WEEKDAYS]} style={{ marginBottom: 0, flex: 1 }}>
                        <WeekdayPicker />
                      </Form.Item>
                    </div>
                    <Text type="secondary" style={{ fontSize: 12 }}>
                      {allDay ? `${daysLabel}全天` : `${daysLabel}：此时段用上面折扣，其余用统一折扣`}
                    </Text>
                  </div>
                );
              })}
              <div style={{ display: 'flex', gap: 8 }}>
                <Button
                  type="dashed"
                  block
                  icon={<PlusOutlined />}
                  onClick={() => add({
                    all_day: false,
                    days: [...ALL_WEEKDAYS],
                    site_discount: siteFallback,
                    global_discount: globalFallback,
                  })}
                >
                  添加时段
                </Button>
                <Button
                  type="dashed"
                  block
                  icon={<PlusOutlined />}
                  onClick={() => {
                    const used = new Set<number>();
                    slots.forEach((item) => {
                      if (isAllDayMultiplier(item)) normalizeWeekdays(item.days).forEach((day) => used.add(day));
                    });
                    const nextDay = [6, 7, 1, 2, 3, 4, 5].find((day) => !used.has(day)) || 7;
                    add({
                      all_day: true,
                      days: [nextDay],
                      site_discount: siteFallback,
                      global_discount: globalFallback,
                    });
                  }}
                >
                  添加某天全天
                </Button>
              </div>
            </>
          )}
        </Form.List>
        {enabled && slots.length > 0 && siteOn && (
          <TimeMultiplierChart
            items={discountChartItems(slots, 'site_discount', siteFallback)}
            invert={false}
            isLight={isLight}
            title="折扣限价"
            description="色块是该时段折扣限价，空白用上方统一限价。"
          />
        )}
        {enabled && slots.length > 0 && globalOn && (
          <TimeMultiplierChart
            items={discountChartItems(slots, 'global_discount', globalFallback)}
            invert={false}
            isLight={isLight}
            title="全站折扣"
            description="色块是该时段全站折扣，空白用上方统一折扣。"
          />
        )}
      </div>
    </div>
  );
};

export default DiscountScheduleEditor;
