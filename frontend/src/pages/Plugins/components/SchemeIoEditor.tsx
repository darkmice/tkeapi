/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia
 * @license        MIT (https://www.tkeapi.com/)
 */

/**
 * 创作方案 IO 配置编辑器（管理端）
 * 口结构固定展示；快捷模板只切换启用/禁用
 */
import React, { useMemo } from 'react';
import {
  AutoComplete,
  Divider,
  Input,
  InputNumber,
  Radio,
  Select,
  Space,
  Switch,
  Tooltip,
  Typography,
} from 'antd';
import { QuestionCircleOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { WORKFLOW_BASIC_NODE_TYPES } from './scheme/workflowBasicNodes';
import {
  fullImageSchemeIoCatalog,
  fullAudioSchemeIoCatalog,
  mergePortsWithCatalog,
  applyIoEnabledKeys,
  IO_TEMPLATE_ENABLE,
  defaultAcceptAssetKinds,
  defaultReferencePortMax,
  isReferenceMediaPort,
  portShowsMaxCount,
  SCHEME_ASSET_KIND_OPTIONS,
  videoSchemeIoCatalogFor,
  isWan3FamilySchemeId,
  isSeedance2SchemeId,
  isMinimaxH3SchemeId,
  isDoubaoSeedEvolvingSchemeId,
  isFeatureTogglePort,
  fullChatSchemeIoCatalog,
  inferSeedanceEdition,
  applySeedanceEditionToInputs,
  applySeedanceEditionToParams,
  inferMinimaxH3Edition,
  applyMinimaxH3EditionToInputs,
  applyMinimaxH3EditionToParams,
  normalizeOmniCapabilityPorts,
  SEEDANCE_EDITION_LIMITS,
  MINIMAX_H3_EDITION,
} from './scheme/schemeIo';
import { localizeSchemePhrase } from './scheme/schemeParamUtils';

const { Text } = Typography;

const ACCEPTS_OPTIONS = WORKFLOW_BASIC_NODE_TYPES.map((t) => ({
  label: t.label,
  value: t.key,
}));

/** 若 inputs/outputs 缺失则按类型铺齐完整目录，并套用默认启用模板 */
export function ensureSchemeIoDefaults(scheme: any): any {
  if (!scheme) return scheme;
  const schemeId = String(scheme.id || '');
  if (scheme.type === 'chat') {
    const catalog = fullChatSchemeIoCatalog();
    const hasInputs = Array.isArray(scheme.inputs) && scheme.inputs.length > 0;
    const hasOutputs = Array.isArray(scheme.outputs) && scheme.outputs.length > 0;
    if (hasInputs && hasOutputs) {
      return {
        ...scheme,
        inputs: mergePortsWithCatalog(catalog.inputs, scheme.inputs),
        outputs: mergePortsWithCatalog(catalog.outputs, scheme.outputs),
      };
    }
    const tpl = isDoubaoSeedEvolvingSchemeId(schemeId)
      ? IO_TEMPLATE_ENABLE.chat_doubao
      : IO_TEMPLATE_ENABLE.chat_default;
    const seeded = applyIoEnabledKeys(catalog, scheme, tpl.inputs, tpl.outputs);
    return {
      ...scheme,
      inputs: hasInputs ? mergePortsWithCatalog(catalog.inputs, scheme.inputs) : seeded.inputs,
      outputs: hasOutputs ? mergePortsWithCatalog(catalog.outputs, scheme.outputs) : seeded.outputs,
    };
  }
  const catalog =
    scheme.type === 'image'
      ? fullImageSchemeIoCatalog()
      : scheme.type === 'audio'
        ? fullAudioSchemeIoCatalog()
        : videoSchemeIoCatalogFor(schemeId);
  const hasInputs = Array.isArray(scheme.inputs) && scheme.inputs.length > 0;
  const hasOutputs = Array.isArray(scheme.outputs) && scheme.outputs.length > 0;

  const isNoNegVideo =
    schemeId === 'openai_video' ||
    isSeedance2SchemeId(schemeId) ||
    isWan3FamilySchemeId(schemeId) ||
    isMinimaxH3SchemeId(schemeId);
  const cleanCurrentInputs = isNoNegVideo && Array.isArray(scheme.inputs)
    ? scheme.inputs.filter((p: any) => p?.key !== 'negative_prompt' && p?.bind_key !== 'negative_prompt')
    : scheme.inputs;

  const wrapInputs = (ports: any[]) =>
    isSeedance2SchemeId(schemeId) || isWan3FamilySchemeId(schemeId)
      ? normalizeOmniCapabilityPorts(ports)
      : ports;

  if (hasInputs && hasOutputs) {
    return {
      ...scheme,
      inputs: wrapInputs(mergePortsWithCatalog(catalog.inputs, cleanCurrentInputs)),
      outputs: mergePortsWithCatalog(catalog.outputs, scheme.outputs),
    };
  }

  const tplKey =
    scheme.type === 'image'
      ? schemeId === 'seedream_5_0_pro'
        ? 'image_seedream_5_0_pro'
        : 'image_default'
      : scheme.type === 'audio'
        ? 'audio_tts'
      : schemeId === 'seedance1.5pro' || schemeId === 'kling_video'
        ? 'video_start_end'
        : schemeId === 'minimax-h3'
          ? 'video_minimax_h3'
          : schemeId === 'dashscope_video' || schemeId === 'wan3.0'
            ? 'video_dashscope_wan3'
            : schemeId === 'seedance2.0' || schemeId === 'seedance2'
              ? 'video_seedance2'
              : schemeId === 'openai_video'
                ? 'video_openai'
                : schemeId.includes('seedance')
                  ? 'video_multimodal'
                  : 'video_text';
  const tpl = IO_TEMPLATE_ENABLE[tplKey] || IO_TEMPLATE_ENABLE.video_openai;
  const seeded = applyIoEnabledKeys(catalog, scheme, tpl.inputs, tpl.outputs);
  return {
    ...scheme,
    inputs: wrapInputs(hasInputs ? mergePortsWithCatalog(catalog.inputs, cleanCurrentInputs) : seeded.inputs),
    outputs: hasOutputs ? mergePortsWithCatalog(catalog.outputs, scheme.outputs) : seeded.outputs,
  };
}

export type SchemeIoEditorProps = {
  schemeType: string;
  params: { key: string; label?: string }[];
  inputs: any[];
  outputs: any[];
  onChange: (next: {
    inputs: any[];
    outputs: any[];
    params?: any[];
    max_reference_images?: number;
  }) => void;
  isLight?: boolean;
  /** 独立抽屉内使用：去掉顶部分隔线 */
  standalone?: boolean;
  schemeId?: string;
  /** 模型 IO 覆写：绑定字段以方案为准，不可改 */
  bindKeyDisabled?: boolean;
  /** 创作中心 2026：口名、节点类型、素材类型随界面语言显示 */
  localizeLabels?: boolean;
};

const SchemeIoEditor: React.FC<SchemeIoEditorProps> = ({
  schemeType,
  params,
  inputs,
  outputs,
  onChange,
  isLight = true,
  standalone = false,
  schemeId,
  bindKeyDisabled = false,
  localizeLabels = false,
}) => {
  const { t } = useTranslation('playground_2026');
  const phrase = (text: string) => (localizeLabels ? localizeSchemePhrase(text) : text);
  const muted = isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)';
  const cardBg = isLight ? '#fafafa' : '#1a1a1a';
  const cardBorder = isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)';
  const fieldHint: React.CSSProperties = {
    display: 'block',
    marginBottom: 4,
    fontSize: 11,
    color: muted,
  };

  const catalog = useMemo(
    () =>
      schemeType === 'image'
        ? fullImageSchemeIoCatalog()
        : schemeType === 'audio'
          ? fullAudioSchemeIoCatalog()
          : schemeType === 'chat'
            ? fullChatSchemeIoCatalog()
            : videoSchemeIoCatalogFor(schemeId),
    [schemeType, schemeId],
  );

  const displayInputs = useMemo(() => {
    const merged = mergePortsWithCatalog(catalog.inputs, inputs);
    return isSeedance2SchemeId(schemeId) || isWan3FamilySchemeId(schemeId)
      ? normalizeOmniCapabilityPorts(merged)
      : merged;
  }, [catalog.inputs, inputs, schemeId]);
  const displayOutputs = useMemo(
    () => mergePortsWithCatalog(catalog.outputs, outputs),
    [catalog.outputs, outputs],
  );
  const seedanceMode = isSeedance2SchemeId(schemeId);
  const wan3Mode = isWan3FamilySchemeId(schemeId);
  const minimaxMode = isMinimaxH3SchemeId(schemeId);
  const omniFamilyMode = seedanceMode || wan3Mode;
  const seedanceEdition = inferSeedanceEdition(displayInputs);
  const minimaxEdition = inferMinimaxH3Edition(displayInputs, params);
  const chatMode = schemeType === 'chat';
  const imageMode = schemeType === 'image';

  const bindOptions = (params || [])
    .filter((p) => p?.key)
    .map((p) => ({
      value: p.key,
      label: p.label ? `${p.key}（${phrase(p.label)}）` : p.key,
    }));

  const commit = (
    nextInputs: any[],
    nextOutputs: any[],
    extra?: { params?: any[]; max_reference_images?: number },
  ) => {
    onChange({ inputs: nextInputs, outputs: nextOutputs, ...extra });
  };

  const patchInput = (idx: number, field: string, value: any) => {
    const next = displayInputs.map((p, i) => (i === idx ? { ...p, [field]: value } : p));
    commit(next, displayOutputs);
  };

  const patchOutput = (idx: number, field: string, value: any) => {
    const next = displayOutputs.map((p, i) => (i === idx ? { ...p, [field]: value } : p));
    commit(displayInputs, next);
  };

  const renderInputCard = (port: any, idx: number, nested: boolean) => {
    const isOn = port.enabled !== false;
    const showMax = portShowsMaxCount(port);
    const maxFallback = defaultReferencePortMax(port.key || '', schemeId);
    return (
      <div
        key={`in-${port.key}`}
        style={{
          background: isOn ? cardBg : (isLight ? 'rgba(0,0,0,0.03)' : 'rgba(255,255,255,0.03)'),
          borderRadius: 8,
          padding: 14,
          border: cardBorder,
          marginBottom: nested ? 8 : 10,
          opacity: isOn ? 1 : 0.55,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
          <Space>
            <Text strong style={{ fontSize: 13, color: isLight ? '#1f2937' : '#fff' }}>
              {phrase(port.label || port.key)}
            </Text>
            <Switch
              size="small"
              checked={isOn}
              onChange={(v) => patchInput(idx, 'enabled', v)}
            />
            <Text style={{ fontSize: 11, color: muted }}>{isOn ? t('admin_enabled', '启用') : t('admin_disabled', '已禁用')}</Text>
          </Space>
        </div>
        {isFeatureTogglePort(port) ? (
          <Text style={{ display: 'block', fontSize: 12, color: muted }}>
            {port.key === 'thinking'
              ? t('admin_io_thinking_tip', '开启后，用户端聊天栏出现深度思考按钮（默认开）。关闭后不出现按钮，请求不带 thinking。模型 IO 可单独覆盖。')
              : t('admin_io_feature_toggle_tip', '能力开关，不是上传口。')}
          </Text>
        ) : (
        <>
        <div style={{ display: 'flex', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
          <div style={{ flex: 1, minWidth: 140 }}>
            <Text style={fieldHint}>{t('admin_io_bind_key', '绑定字段')}</Text>
            <AutoComplete
              size="small"
              style={{ width: '100%' }}
              options={bindOptions}
              value={port.bind_key || ''}
              onChange={(v) => patchInput(idx, 'bind_key', v)}
              placeholder={t('admin_io_bind_ph', '选参数键或手填')}
              disabled={!isOn || bindKeyDisabled}
              filterOption={(input, option) =>
                String(option?.value || '')
                  .toLowerCase()
                  .includes(input.toLowerCase()) ||
                String(option?.label || '')
                  .toLowerCase()
                  .includes(input.toLowerCase())
              }
            />
          </div>
          <div style={{ flex: 2, minWidth: 180 }}>
            <Text style={fieldHint}>{t('admin_io_upstream', '可连接上游')}</Text>
            <Select
              size="small"
              mode="multiple"
              style={{ width: '100%' }}
              options={ACCEPTS_OPTIONS.map((opt) => ({ ...opt, label: phrase(opt.label) }))}
              value={port.accepts || []}
              onChange={(v) => {
                const list = Array.isArray(v) ? v : [];
                const hasAsset = list.includes('asset');
                const next = displayInputs.map((p, i) => {
                  if (i !== idx) return p;
                  return {
                    ...p,
                    accepts: list,
                    accept_asset_kinds: hasAsset
                      ? p.accept_asset_kinds?.length
                        ? p.accept_asset_kinds
                        : defaultAcceptAssetKinds(p.modality)
                      : undefined,
                  };
                });
                commit(next, displayOutputs);
              }}
              placeholder={t('admin_io_nodes_ph', '选择节点类型')}
              disabled={!isOn}
            />
          </div>
        </div>
        {(port.accepts || []).includes('asset') && (
          <div style={{ marginBottom: 8 }}>
            <Text style={fieldHint}>
              {t('admin_io_asset_type', '素材类型')}{' '}
              <Tooltip title={t('admin_io_asset_tip', '勾选「素材」后生效：限制可接入的素材文件类型。未选时按口模态（图/视/音）回落。')}>
                <QuestionCircleOutlined style={{ color: muted, cursor: 'help' }} />
              </Tooltip>
            </Text>
            <Select
              size="small"
              mode="multiple"
              style={{ width: '100%', maxWidth: 420 }}
              options={SCHEME_ASSET_KIND_OPTIONS.map((opt) => ({ ...opt, label: phrase(opt.label) }))}
              value={
                port.accept_asset_kinds?.length
                  ? port.accept_asset_kinds
                  : defaultAcceptAssetKinds(port.modality)
              }
              onChange={(v) =>
                patchInput(
                  idx,
                  'accept_asset_kinds',
                  Array.isArray(v) && v.length ? v : defaultAcceptAssetKinds(port.modality),
                )
              }
              placeholder={t('admin_io_assets_ph', '选择素材文件类型')}
              disabled={!isOn}
            />
          </div>
        )}
        {showMax && (
          <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
            <div style={{ width: 140 }}>
              <Text style={fieldHint}>
                {t('admin_io_max_count', '数量上限')}{' '}
                <Tooltip title={imageMode && isReferenceMediaPort(port)
                  ? t('admin_io_image_max_tip', '数量上限为 1 时是图文生图，大于 1 时是多图文生图。同步限制用户端提示词可附的参考图数量。')
                  : t('admin_io_max_tip', '工作流与生成工具中，该口可上传或接入的最大数量。参考图/参考视频/参考音频会同步限制用户端提示词附件。编辑视频与延长视频默认为 1。')}>
                  <QuestionCircleOutlined style={{ color: muted, cursor: 'help' }} />
                </Tooltip>
              </Text>
              <InputNumber
                size="small"
                min={0}
                max={64}
                precision={0}
                style={{ width: '100%' }}
                value={port.max ?? maxFallback}
                onChange={(v) => patchInput(idx, 'max', typeof v === 'number' ? v : maxFallback)}
                disabled={!isOn}
              />
            </div>
          </div>
        )}
        </>
        )}
      </div>
    );
  };

  const inputBlocks: Array<
    { type: 'single'; idx: number } | { type: 'group'; title: string; indices: number[] }
  > = [];
  {
    const used = new Set<number>();
    const refIndices = displayInputs
      .map((p, i) => (isReferenceMediaPort(p) ? i : -1))
      .filter((i) => i >= 0);
    displayInputs.forEach((port, idx) => {
      if (used.has(idx)) return;
      if (omniFamilyMode && isFeatureTogglePort(port)) {
        used.add(idx);
        return;
      }
      if (isReferenceMediaPort(port) && refIndices.length) {
        refIndices.forEach((i) => used.add(i));
        inputBlocks.push({
          type: 'group',
          title: imageMode
            ? t('admin_io_image_ref_group', '图文生图 / 多图文生图')
            : t('admin_io_ref_i2v', '参考生视频'),
          indices: refIndices,
        });
        return;
      }
      used.add(idx);
      inputBlocks.push({ type: 'single', idx });
    });
  }

  return (
    <div>
      {!standalone && (
        <Divider style={{ margin: '8px 0', borderColor: isLight ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)' }} />
      )}

      <div style={{ marginBottom: 8 }}>
        <Text strong style={{ color: isLight ? '#1f2937' : '#fff', fontSize: 14 }}>
          {standalone ? t('admin_scheme_io_title', '输入输出配置') : t('admin_io_title', 'IO 配置')}
        </Text>
      </div>
      <Text style={{ display: 'block', marginTop: 0, marginBottom: 12, fontSize: 12, color: muted }}>
        {seedanceMode
          ? t('admin_io_seedance_intro', 'Seedance 按官方能力配置：文生 / 首帧 / 首尾帧 / 全能参考。编辑与延长是全能参考的子能力，默认开启。')
          : wan3Mode
            ? t('admin_io_wan3_intro', '万相 3.0 按官方能力配置：文生 / 首帧 / 首尾帧 / 全能参考。编辑与延长是全能参考的子能力，默认开启。')
            : minimaxMode
              ? t('admin_io_minimax_intro', 'MiniMax H3 按官方能力配置：文生 / 图生（首尾帧）/ 多模态参考。可切换 MiniMax-H3 与 MiniMax-H3-Max。')
            : chatMode
              ? t('admin_io_chat_thinking_intro', '深度思考是能力开关：打开后用户端聊天栏显示深度思考按钮。关闭则不显示、请求不带 thinking。模型 IO 可单独覆盖方案。')
              : imageMode
                ? t('admin_io_image_intro', '图片方案按文生图、图文生图、多图文生图配置。提示词是文生图入口；参考图关闭为文生图，数量上限为 1 是图文生图，大于 1 是多图文生图。')
                : t('admin_io_generic_intro', '全部口固定展示；关闭「启用」后其余字段不可改。方案列表点「重置」可恢复默认参数与 IO。')}
      </Text>

      <div style={{ marginBottom: 8 }}>
        <Text style={{ fontSize: 13, color: isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{t('admin_io_inputs', '输入')}</Text>
      </div>

      {inputBlocks.map((block) => {
        if (block.type === 'group') {
          const groupOn = block.indices.some((i) => displayInputs[i]?.enabled !== false);
          const editPort = displayInputs.find((p) => p.key === 'edit_video');
          const extendPort = displayInputs.find((p) => p.key === 'extend_video');
          const lim = SEEDANCE_EDITION_LIMITS[seedanceEdition];
          return (
            <div
              key="in-group-reference-i2v"
              style={{
                borderRadius: 10,
                padding: 12,
                marginBottom: 10,
                border: isLight ? '1px solid rgba(22,119,255,0.18)' : '1px solid rgba(255,255,255,0.12)',
                background: isLight ? 'rgba(22,119,255,0.03)' : 'rgba(255,255,255,0.03)',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8 }}>
                <Space wrap>
                  <Text strong style={{ fontSize: 13, color: isLight ? '#1f2937' : '#fff' }}>
                    {block.title}
                  </Text>
                  <Switch
                    size="small"
                    checked={groupOn}
                    onChange={(v) => {
                      if (minimaxMode && minimaxEdition === 'h3-max' && v) {
                        commit(
                          applyMinimaxH3EditionToInputs(displayInputs, 'h3'),
                          displayOutputs,
                          {
                            params: applyMinimaxH3EditionToParams(params || [], 'h3'),
                            max_reference_images: MINIMAX_H3_EDITION.h3.images,
                          },
                        );
                        return;
                      }
                      const next = displayInputs.map((p, i) => {
                        if (block.indices.includes(i)) return { ...p, enabled: v };
                        if (omniFamilyMode && (p.key === 'edit_video' || p.key === 'extend_video')) {
                          return { ...p, enabled: v };
                        }
                        return p;
                      });
                      commit(next, displayOutputs);
                    }}
                  />
                  <Text style={{ fontSize: 11, color: muted }}>{groupOn ? t('admin_enabled', '启用') : t('admin_disabled', '已禁用')}</Text>
                </Space>
              </div>
              {minimaxMode && (
                <div style={{ marginBottom: 10 }}>
                  <Radio.Group
                    size="small"
                    value={minimaxEdition}
                    onChange={(e) => {
                      const edition = e.target.value as 'h3' | 'h3-max';
                      commit(
                        applyMinimaxH3EditionToInputs(displayInputs, edition),
                        displayOutputs,
                        {
                          params: applyMinimaxH3EditionToParams(params || [], edition),
                          max_reference_images: edition === 'h3-max' ? 0 : MINIMAX_H3_EDITION.h3.images,
                        },
                      );
                    }}
                  >
                    <Radio.Button value="h3">MiniMax-H3</Radio.Button>
                    <Radio.Button value="h3-max">MiniMax-H3-Max</Radio.Button>
                  </Radio.Group>
                  <Text style={{ display: 'block', marginTop: 6, fontSize: 11, color: muted }}>
                    {minimaxEdition === 'h3-max'
                      ? t('admin_io_minimax_max_rule', '仅文生 / 图生（首尾帧）；不支持参考图、参考视频、参考音频。分辨率 480P / 768P，时长 5–15 秒')
                      : t('admin_io_minimax_h3_rule', '文生 / 图生（首尾帧）/ 多模态参考。参考图 0–9、视频 0–3、音频 0–3。分辨率 768P / 2K，时长 4–15 秒')}
                  </Text>
                </div>
              )}
              {seedanceMode && (
                <div style={{ marginBottom: 10 }}>
                  <Radio.Group
                    size="small"
                    value={seedanceEdition}
                    onChange={(e) => {
                      const edition = e.target.value as '2.0' | '2.5';
                      commit(
                        applySeedanceEditionToInputs(displayInputs, edition),
                        displayOutputs,
                        {
                          params: applySeedanceEditionToParams(params || [], edition),
                          max_reference_images: SEEDANCE_EDITION_LIMITS[edition].images,
                        },
                      );
                    }}
                  >
                    <Radio.Button value="2.0">Seedance 2.0</Radio.Button>
                    <Radio.Button value="2.5">Seedance 2.5</Radio.Button>
                  </Radio.Group>
                  <Text style={{ display: 'block', marginTop: 6, fontSize: 11, color: muted }}>
                    {seedanceEdition === '2.5'
                      ? t('admin_io_seedance_25_rule', '参考图 0–{{images}}、视频 0–{{videos}}、音频 0–{{audios}}；可任意搭配（含只传音频）；无参考素材按文生视频；时长最长 {{durationMax}} 秒', {
                        images: lim.images,
                        videos: lim.videos,
                        audios: lim.audios,
                        durationMax: lim.durationMax,
                      })
                      : t('admin_io_seedance_20_rule', '参考图 0–{{images}}、视频 0–{{videos}}、音频 0–{{audios}}；无参考素材按文生视频；传了参考音频必须搭配参考图', {
                        images: lim.images,
                        videos: lim.videos,
                        audios: lim.audios,
                      })}
                  </Text>
                </div>
              )}
              {wan3Mode && (
                <Text style={{ display: 'block', marginBottom: 10, fontSize: 11, color: muted }}>
                  {t('admin_io_wan3_ref_rule', '参考图 0–10、视频 0–5、音频 0–5；图/视频/音频可任意组合（含只传音频）。有参考素材时比例建议 adaptive；编辑时长建议 -1。')}
                </Text>
              )}
              {!omniFamilyMode && !minimaxMode && (
                <Text style={{ display: 'block', marginBottom: 10, fontSize: 11, color: muted }}>
                  {imageMode
                    ? t('admin_io_image_ref_group_rule', '关闭参考图为文生图。数量上限为 1 是图文生图；大于 1 是多图文生图。')
                    : t('admin_io_ref_group_rule', '参考图、参考视频、参考音频同属一组功能，可单独开关并设置数量上限。')}
                </Text>
              )}
              {block.indices.map((idx) => renderInputCard(displayInputs[idx], idx, true))}
              {omniFamilyMode && editPort && (
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    padding: '10px 0 4px',
                    borderTop: isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)',
                    marginTop: 4,
                  }}
                >
                  <div>
                    <Text strong style={{ fontSize: 13, color: isLight ? '#1f2937' : '#fff' }}>{t('admin_io_edit_video', '编辑视频')}</Text>
                    <Text style={{ display: 'block', fontSize: 11, color: muted }}>
                      {t('admin_io_edit_keep', '自动保持输出宽高比、时长与待编辑视频一致')}
                    </Text>
                  </div>
                  <Switch
                    size="small"
                    checked={editPort.enabled !== false}
                    onChange={(v) => {
                      const next = displayInputs.map((p) =>
                        p.key === 'edit_video' ? { ...p, enabled: v } : p,
                      );
                      commit(next, displayOutputs);
                    }}
                  />
                </div>
              )}
              {omniFamilyMode && extendPort && (
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    padding: '10px 0 2px',
                  }}
                >
                  <div>
                    <Text strong style={{ fontSize: 13, color: isLight ? '#1f2937' : '#fff' }}>{t('admin_io_extend_video', '延长视频')}</Text>
                    <Text style={{ display: 'block', fontSize: 11, color: muted }}>
                      {t('admin_io_extend_keep', '自动保持输出宽高比与待延长视频一致')}
                    </Text>
                  </div>
                  <Switch
                    size="small"
                    checked={extendPort.enabled !== false}
                    onChange={(v) => {
                      const next = displayInputs.map((p) =>
                        p.key === 'extend_video' ? { ...p, enabled: v } : p,
                      );
                      commit(next, displayOutputs);
                    }}
                  />
                </div>
              )}
            </div>
          );
        }
        return renderInputCard(displayInputs[block.idx], block.idx, false);
      })}

      <div style={{ margin: '16px 0 8px' }}>
        <Text style={{ fontSize: 13, color: isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>{t('admin_io_outputs', '输出')}</Text>
      </div>

      {displayOutputs.map((port: any, idx: number) => {
        const isOn = port.enabled !== false;
        return (
        <div
          key={`out-${port.key}`}
          style={{
            background: isOn ? cardBg : (isLight ? 'rgba(0,0,0,0.03)' : 'rgba(255,255,255,0.03)'),
            borderRadius: 8,
            padding: 14,
            border: cardBorder,
            marginBottom: 10,
            opacity: isOn ? 1 : 0.55,
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
            <Space>
              <Text strong style={{ fontSize: 13, color: isLight ? '#1f2937' : '#fff' }}>
                {phrase(port.label || port.key)}
              </Text>
              <Switch
                size="small"
                checked={isOn}
                onChange={(v) => patchOutput(idx, 'enabled', v)}
              />
              <Text style={{ fontSize: 11, color: muted }}>{isOn ? t('admin_enabled', '启用') : t('admin_disabled', '已禁用')}</Text>
            </Space>
          </div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <div style={{ flex: 1, minWidth: 140 }}>
              <Text style={fieldHint}>{t('admin_io_result_field', '结果字段')}</Text>
              <Input
                size="small"
                value={port.result_key || ''}
                onChange={(e) => patchOutput(idx, 'result_key', e.target.value)}
                placeholder={t('admin_io_result_ph', '如 image_url / video_url')}
                disabled={!isOn || bindKeyDisabled}
              />
            </div>
          </div>
        </div>
        );
      })}
    </div>
  );
};

export default SchemeIoEditor;
