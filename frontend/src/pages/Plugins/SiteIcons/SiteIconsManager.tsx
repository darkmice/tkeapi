/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { Typography, Input, Button, Space, Spin, message, Tag, Modal, Form, Select, Upload, Tooltip, Empty, Popconfirm, Progress, ColorPicker, InputNumber } from 'antd';
import { PlusOutlined, SearchOutlined, DeleteOutlined, EditOutlined, CloudDownloadOutlined, HistoryOutlined, InboxOutlined, CloseOutlined, ClearOutlined } from '@ant-design/icons';
import request from '../../../utils/request';
import { useThemeStore } from '../../../store/theme';
import { formatApiDateTime } from '../../../utils/timedisplay';
import PluginLogRetentionCard from '../components/PluginLogRetentionCard';
import ListPagination, { useListPager } from '../../../components/ListPagination';
import { applySvgEdits, fitSvgPreview, readSvgMeta, type SvgEdits } from './svgEdit';

const ICON_PAGE_SIZE = 100;
const NAME_INITIALS = 'abcdefghijklmnopqrstuvwxyz'.split('');

const { Title, Text } = Typography;
const { TextArea } = Input;
const { Dragger } = Upload;

interface SiteIcon {
  id: number; name: string; title: string; file_path: string;
  source: string; category: string; tags: string; is_active: number;
  created_at: string; updated_at: string;
}
interface SyncLog {
  id: number; total_synced: number; total_new: number; total_updated: number;
  status: string; error_message: string | null; created_at: string;
}
interface StorageStatus {
  success: boolean;
  provider: string;
  is_cloud: boolean;
  bucket?: string;
  endpoint?: string;
  region?: string;
  path_prefix?: string;
}

const SiteIconsManager: React.FC = () => {
  const { themeMode } = useThemeStore();
  const _isLight = themeMode === 'light';
  const [allIcons, setAllIcons] = useState<SiteIcon[]>([]);
  const { page, pageSize, setPage, setPageSize } = useListPager(ICON_PAGE_SIZE);
  const [loading, setLoading] = useState(false);
  const [searchKeyword, setSearchKeyword] = useState('');
  const [filterCategory, setFilterCategory] = useState('');
  const [filterSource, setFilterSource] = useState('');
  const [nameInitial, setNameInitial] = useState('');
  const [storageInfo, setStorageInfo] = useState<StorageStatus | null>(null);

  // 同步状态
  const [syncing, setSyncing] = useState(false);
  const [syncLogs, setSyncLogs] = useState<string[]>([]);
  const [syncTotal, setSyncTotal] = useState(0);
  const [syncCurrent, setSyncCurrent] = useState(0);
  const [syncFinished, setSyncFinished] = useState(false);
  const [showSyncPanel, setShowSyncPanel] = useState(false);
  const logOffsetRef = useRef(0);
  const pollTimerRef = useRef<ReturnType<typeof setInterval>>(undefined);
  const logPanelRef = useRef<HTMLDivElement>(null);

  // Modal 状态
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [editingIcon, setEditingIcon] = useState<SiteIcon | null>(null);
  const [form] = Form.useForm();
  const [svgPreview, setSvgPreview] = useState('');
  const [svgOriginal, setSvgOriginal] = useState('');
  const [svgWidth, setSvgWidth] = useState(24);
  const [svgHeight, setSvgHeight] = useState(24);
  const [svgBg, setSvgBg] = useState<string | null>(null);
  const [svgFill, setSvgFill] = useState<string | null>(null);
  const [svgLoading, setSvgLoading] = useState(false);
  const svgBaseRef = useRef('');
  const editsRef = useRef<SvgEdits>({ width: 24, height: 24, background: null, iconColor: null });
  const [saving, setSaving] = useState(false);
  const [logModalVisible, setLogModalVisible] = useState(false);
  const [syncLogList, setSyncLogList] = useState<SyncLog[]>([]);
  const [logsLoading, setLogsLoading] = useState(false);
  const [previewIcon, setPreviewIcon] = useState<SiteIcon | null>(null);
  const [resetting, setResetting] = useState(false);

  const fetchStorageStatus = useCallback(async () => {
    try {
      const res = await (request.get('/plugins/site-icons/storage-status') as any);
      if (res?.data || res?.success) {
        setStorageInfo(res.data || res);
      }
    } catch {
      // silent
    }
  }, []);

  useEffect(() => {
    fetchStorageStatus();
  }, [fetchStorageStatus]);

  const fetchIcons = useCallback(async () => {
    try {
      setLoading(true);
      const batchSize = 200;
      let reqPage = 1;
      let loaded: SiteIcon[] = [];
      let totalCount = 0;
      for (;;) {
        const params: any = { page: reqPage, size: batchSize };
        if (searchKeyword) params.q = searchKeyword;
        if (filterCategory) params.category = filterCategory;
        if (filterSource) params.source = filterSource;
        const res = await (request.get('/plugins/site-icons', { params }) as any);
        const batch: SiteIcon[] = res.data || [];
        totalCount = res.total ?? loaded.length + batch.length;
        loaded = loaded.concat(batch);
        if (batch.length === 0 || loaded.length >= totalCount || reqPage >= 50) break;
        reqPage += 1;
      }
      loaded.sort((a, b) => a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }));
      setAllIcons(loaded);
    } catch (e) { console.error(e); }
    finally { setLoading(false); }
  }, [searchKeyword, filterCategory, filterSource]);

  useEffect(() => { fetchIcons(); }, [fetchIcons]);

  useEffect(() => { setPage(1); }, [searchKeyword, filterCategory, filterSource, setPage]);

  const filteredIcons = useMemo(() => {
    if (!nameInitial) return allIcons;
    if (nameInitial === '#') {
      return allIcons.filter(icon => !/^[a-z]/i.test(icon.name || ''));
    }
    const letter = nameInitial.toLowerCase();
    return allIcons.filter(icon => (icon.name || '').toLowerCase().startsWith(letter));
  }, [allIcons, nameInitial]);

  const pagedIcons = useMemo(() => {
    const start = (page - 1) * pageSize;
    return filteredIcons.slice(start, start + pageSize);
  }, [filteredIcons, page, pageSize]);

  // ── 同步轮询 ──
  const pollProgress = useCallback(async () => {
    try {
      const res = await (request.get('/plugins/site-icons/sync-progress', {
        params: { since: logOffsetRef.current }
      }) as any);
      const d = res.data;
      if (d.logs?.length) {
        setSyncLogs(prev => [...prev, ...d.logs]);
        logOffsetRef.current = d.log_offset;
        // 自动滚动到底部
        setTimeout(() => {
          if (logPanelRef.current) {
            logPanelRef.current.scrollTop = logPanelRef.current.scrollHeight;
          }
        }, 50);
      }
      if (d.total) setSyncTotal(d.total);
      if (d.current != null) setSyncCurrent(d.current);
      if (d.finished) {
        setSyncFinished(true);
        setSyncing(false);
        if (pollTimerRef.current) { clearInterval(pollTimerRef.current); pollTimerRef.current = undefined; }
        fetchIcons();
      }
    } catch { /* silent */ }
  }, [fetchIcons]);

  const handleSync = async () => {
    try {
      setSyncing(true);
      setSyncFinished(false);
      setSyncLogs([]);
      setSyncTotal(0);
      setSyncCurrent(0);
      logOffsetRef.current = 0;
      setShowSyncPanel(true);
      await (request.post('/plugins/site-icons/sync', {}) as any);
      // 开始轮询
      pollTimerRef.current = setInterval(pollProgress, 1500);
    } catch (e: any) {
      setSyncing(false);
      console.error(e);
    }
  };

  useEffect(() => {
    return () => { if (pollTimerRef.current) clearInterval(pollTimerRef.current); };
  }, []);

  // ── CRUD ──
  const handleResetLibrary = () => {
    Modal.confirm({
      title: '清空重置图标库',
      content: '将永久删除图标库中的全部图标，包括数据库记录、本地文件和云端文件，不保留任何图标数据。此操作无法恢复。',
      okText: '确认清空',
      cancelText: '取消',
      okButtonProps: { danger: true },
      onOk: async () => {
        setResetting(true);
        try {
          const res = await (request.post('/plugins/site-icons/reset', {}) as any);
          if ((res?.local_failed || 0) + (res?.cloud_failed || 0) > 0) {
            message.warning(res.message || '图标记录已清空，部分文件删除失败');
          } else {
            message.success('图标库已全部清空');
          }
          setAllIcons([]);
          fetchIcons();
        } catch (e) {
          console.error(e);
          return Promise.reject(e);
        } finally {
          setResetting(false);
        }
      },
    });
  };

  const handleDelete = async (id: number) => {
    try {
      await request.delete(`/plugins/site-icons/${id}`);
      message.success('已删除');
      fetchIcons();
    } catch (e: any) {
      console.error(e);
      const errMsg = e?.response?.data?.message || e?.message || '删除失败';
      message.error(errMsg);
    }
  };
  const absorbSvg = (text: string, replaceOriginal: boolean) => {
    const meta = readSvgMeta(text);
    const edits: SvgEdits = { width: meta.width, height: meta.height, background: meta.background, iconColor: null };
    svgBaseRef.current = text;
    editsRef.current = edits;
    if (replaceOriginal) setSvgOriginal(text);
    setSvgPreview(text);
    setSvgWidth(meta.width);
    setSvgHeight(meta.height);
    setSvgBg(meta.background);
    setSvgFill(null);
  };
  const patchSvg = (partial: Partial<SvgEdits>) => {
    const next: SvgEdits = { ...editsRef.current, ...partial };
    editsRef.current = next;
    if (partial.width != null) setSvgWidth(partial.width);
    if (partial.height != null) setSvgHeight(partial.height);
    if ('background' in partial) setSvgBg(partial.background ?? null);
    if ('iconColor' in partial) setSvgFill(partial.iconColor ?? null);
    const base = svgBaseRef.current;
    if (!base.includes('<svg')) return;
    setSvgPreview(applySvgEdits(base, next));
  };
  const handleOpenAdd = () => {
    if (!storageInfo?.is_cloud) {
      message.warning('请配置站点存储才可以添加图标');
      return;
    }
    setEditingIcon(null);
    form.resetFields();
    absorbSvg('', true);
    setSvgLoading(false);
    setAddModalVisible(true);
  };
  const handleOpenEdit = (icon: SiteIcon) => {
    setEditingIcon(icon);
    let tags: string[] = [];
    try { tags = JSON.parse(icon.tags || '[]'); } catch {}
    form.setFieldsValue({ name: icon.name, title: icon.title, category: icon.category, tags });
    absorbSvg('', true);
    setSvgLoading(true);
    setAddModalVisible(true);
    const loadFromFile = async () => {
      const resp = await fetch(getSvgUrl(icon));
      const text = (await resp.text()).trim();
      if (!resp.ok || !text.includes('<svg')) throw new Error('svg');
      absorbSvg(text, true);
    };
    request.get(`/plugins/site-icons/${icon.id}/content`, { skipErrorHandler: true } as any)
      .then(async (res: any) => {
        const text = typeof res?.data === 'string' ? res.data.trim() : '';
        if (!text.includes('<svg')) {
          await loadFromFile();
          return;
        }
        absorbSvg(text, true);
      })
      .catch(() => loadFromFile())
      .catch(() => {
        message.warning('未能读取 SVG 源码，可手动粘贴');
      })
      .finally(() => setSvgLoading(false));
  };
  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      setSaving(true);
      if (editingIcon) {
        const payload: any = { name: values.name, title: values.title, category: values.category, tags: values.tags || [] };
        const nextSvg = svgPreview.trim();
        if (nextSvg && nextSvg !== svgOriginal.trim()) payload.svg_content = nextSvg;
        await request.put(`/plugins/site-icons/${editingIcon.id}`, payload);
        message.success('图标更新成功');
      } else {
        if (!svgPreview.trim()) {
          message.warning('请上传或粘贴 SVG 内容');
          setSaving(false);
          return;
        }
        await request.post('/plugins/site-icons', {
          name: values.name,
          title: values.title,
          category: values.category,
          tags: values.tags || [],
          svg_content: svgPreview.trim()
        });
        message.success('图标添加成功');
      }
      setAddModalVisible(false);
      fetchIcons();
    } catch (e: any) {
      console.error(e);
      const errMsg = e?.response?.data?.message || e?.message || '保存失败';
      message.error(errMsg);
    } finally {
      setSaving(false);
    }
  };
  const handleSvgUpload = (file: File) => {
    const reader = new FileReader();
    reader.onload = (e) => {
      const c = e.target?.result as string;
      if (c && c.includes('<svg')) {
        absorbSvg(c.trim(), !editingIcon);
        message.success('SVG 文件读取成功');
      } else {
        message.error('无效的 SVG 文件，必须包含 <svg> 根标签');
      }
    };
    reader.readAsText(file);
    return false;
  };
  const fetchSyncLogs = async () => {
    try {
      setLogsLoading(true);
      const res = await (request.get('/plugins/site-icons/sync-logs') as any);
      if (res.data) setSyncLogList(res.data);
    } catch (e) {
      console.error(e);
    } finally {
      setLogsLoading(false);
    }
  };
  const getSvgUrl = (icon: SiteIcon) => {
    if (!icon?.name) return '';
    return `/assets/icons/lobe/${encodeURIComponent(icon.name)}.svg?v=${encodeURIComponent(icon.updated_at || '')}`;
  };

  const syncPercent = syncTotal > 0 ? Math.round((syncCurrent / syncTotal) * 100) : 0;
  const iconTileStyle: React.CSSProperties = {
    width: 48, height: 48, display: 'flex', alignItems: 'center', justifyContent: 'center',
    background: _isLight ? 'rgba(0,0,0,0.02)' : 'rgba(255,255,255,0.04)', borderRadius: 8, overflow: 'hidden',
  };
  const iconImgStyle: React.CSSProperties = { maxWidth: 36, maxHeight: 36, objectFit: 'contain' };

  return (
    <div>
      {/* ════ 实时同步日志面板 ════ */}
      {showSyncPanel && (
        <div style={{
          marginBottom: 16, borderRadius: 8, overflow: 'hidden',
          border: syncFinished ? '1px solid rgba(82,196,26,0.3)' : '1px solid rgba(22,119,255,0.3)',
          background: '#0d0d0d' }}>
          {/* 头部 */}
          <div style={{
            display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            padding: '10px 16px',
            background: syncFinished ? 'rgba(82,196,26,0.06)' : 'rgba(22,119,255,0.06)',
            borderBottom: '1px solid rgba(255,255,255,0.06)' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              {syncing && <Spin size="small" />}
              <Text style={{ color: _isLight ? '#1f2937' : '#fff', fontSize: 13, fontWeight: 500 }}>
                {syncing ? '正在同步图标库...' : syncFinished ? '✅ 同步完成' : '同步日志'}
              </Text>
              {syncTotal > 0 && (
                <Tag style={{ margin: 0, fontSize: 11, background: 'rgba(22,119,255,0.1)', border: '1px solid rgba(22,119,255,0.3)', color: '#1677ff' }}>
                  {syncCurrent}/{syncTotal}
                </Tag>
              )}
            </div>
            <Button type="text" size="small" icon={<CloseOutlined />}
              style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)' }}
              onClick={() => { setShowSyncPanel(false); if (pollTimerRef.current) { clearInterval(pollTimerRef.current); pollTimerRef.current = undefined; } }}
            />
          </div>
          {/* 进度条 */}
          {syncTotal > 0 && (
            <div style={{ padding: '6px 16px 0' }}>
              <Progress percent={syncPercent} size="small" strokeColor={syncFinished ? '#52c41a' : '#1677ff'} trailColor="rgba(255,255,255,0.06)" />
            </div>
          )}
          {/* 日志输出 */}
          <div ref={logPanelRef} style={{
            maxHeight: 240, overflow: 'auto', padding: '8px 16px 12px',
            fontFamily: 'Menlo, Monaco, Consolas, monospace', fontSize: 12,
            lineHeight: 1.7, color: _isLight ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.6)' }}>
            {syncLogs.map((log, i) => (
              <div key={i} style={{
                color: log.startsWith('❌') ? '#ff4d4f' : log.startsWith('⚠️') ? '#faad14'
                  : log.startsWith('✅') ? 'rgba(82,196,26,0.8)' : log.startsWith('🎉') ? '#52c41a'
                  : log.startsWith('⏭️') ? 'rgba(255,255,255,0.3)' : 'rgba(255,255,255,0.55)' }}>
                {log}
              </div>
            ))}
            {syncLogs.length === 0 && syncing && (
              <Text style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)' }}>等待服务器响应...</Text>
            )}
          </div>
        </div>
      )}

      {/* ════ 工具栏 ════ */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16, flexWrap: 'wrap', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
          <Input prefix={<SearchOutlined style={{ color: _isLight ? 'rgba(0,0,0,0.25)' : 'rgba(255,255,255,0.25)' }} />} placeholder="搜索图标名称..."
            value={searchKeyword} onChange={e => setSearchKeyword(e.target.value)}
            style={{ width: 240 }} allowClear />
          <Select placeholder="分类" value={filterCategory || undefined} onChange={v => setFilterCategory(v || '')} allowClear
            style={{ width: 130 }} options={[{ label: 'AI品牌', value: 'AI品牌' }, { label: '自定义', value: '自定义' }]} />
          <Select placeholder="来源" value={filterSource || undefined} onChange={v => setFilterSource(v || '')} allowClear
            style={{ width: 130 }} options={[{ label: 'Lobe Icons', value: 'lobe-icons' }, { label: '手动添加', value: 'custom' }]} />
        </div>
        <Space>
          <Tooltip title="永久删除全部图标、本地文件和云端文件，不保留任何数据">
            <span>
              <Button danger icon={<ClearOutlined />} loading={resetting} disabled={syncing || resetting} onClick={handleResetLibrary}>
                清空重置
              </Button>
            </span>
          </Tooltip>
          <Tooltip title="查看同步日志"><Button icon={<HistoryOutlined />} onClick={() => { setLogModalVisible(true); fetchSyncLogs(); }} /></Tooltip>
          <Tooltip title={storageInfo?.is_cloud ? '上传到站点对象存储，并在本地留一份给编辑和模型广场' : '请配置站点存储才可以添加图标'}>
            <Button icon={<PlusOutlined />} onClick={handleOpenAdd}>手动添加</Button>
          </Tooltip>
          <Button type="primary" icon={<CloudDownloadOutlined />} loading={syncing} onClick={handleSync} disabled={syncing}>
            {syncing ? '同步中...' : '在线更新'}
          </Button>
        </Space>
      </div>

      <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 6, marginBottom: 12 }}>
        <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, marginRight: 4 }}>首字母</Text>
        <Button size="small" type={nameInitial === '' ? 'primary' : 'default'} onClick={() => { setNameInitial(''); setPage(1); }}>全部</Button>
        {NAME_INITIALS.map(ch => (
          <Button key={ch} size="small" type={nameInitial === ch ? 'primary' : 'default'} onClick={() => { setNameInitial(ch); setPage(1); }} style={{ width: 28, padding: 0 }}>
            {ch.toUpperCase()}
          </Button>
        ))}
        <Button size="small" type={nameInitial === '#' ? 'primary' : 'default'} onClick={() => { setNameInitial('#'); setPage(1); }}>#</Button>
      </div>

      <Text style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 13, display: 'block', marginBottom: 16 }}>
        共 <span style={{ color: _isLight ? '#1f2937' : '#fff', fontWeight: 500 }}>{filteredIcons.length}</span> 个图标
      </Text>

      {/* ════ 图标网格 ════ */}
      {loading && allIcons.length === 0 ? (
        <div style={{ textAlign: 'center', padding: 80 }}><Spin size="large" /></div>
      ) : filteredIcons.length === 0 ? (
        <Empty description={<Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)' }}>{nameInitial ? '该首字母下暂无图标' : '暂无图标，点击「在线更新」从 Lobe Icons 同步'}</Text>} style={{ padding: 60 }} />
      ) : (
        <>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(110px, 1fr))', gap: 12 }}>
            {pagedIcons.map(icon => (
              <div key={icon.id} style={{
                background: _isLight ? '#fff' : '#141414', border: _isLight ? '1px solid rgba(0,0,0,0.08)' : '1px solid rgba(255,255,255,0.08)', borderRadius: 8,
                padding: '14px 10px 10px', display: 'flex', flexDirection: 'column', alignItems: 'center',
                cursor: 'pointer', transition: 'border-color 0.2s, box-shadow 0.2s', position: 'relative' }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = icon.source === 'custom' ? 'rgba(82,196,26,0.5)' : 'rgba(22,119,255,0.5)'; e.currentTarget.style.boxShadow = '0 0 12px rgba(22,119,255,0.08)'; const a = e.currentTarget.querySelector('.icon-actions') as HTMLElement; if (a) { a.style.opacity = '1'; a.style.width = 'auto'; a.style.overflow = 'visible'; a.style.pointerEvents = 'auto'; } }}
                onMouseLeave={e => { e.currentTarget.style.borderColor = 'rgba(255,255,255,0.08)'; e.currentTarget.style.boxShadow = 'none'; const a = e.currentTarget.querySelector('.icon-actions') as HTMLElement; if (a) { a.style.opacity = '0'; a.style.width = '0'; a.style.overflow = 'hidden'; a.style.pointerEvents = 'none'; } }}
                onClick={() => setPreviewIcon(icon)}
              >
                {icon.source === 'custom' && <Tag style={{ position: 'absolute', top: 4, right: 4, fontSize: 10, lineHeight: '16px', padding: '0 4px', background: 'rgba(82,196,26,0.1)', border: '1px solid rgba(82,196,26,0.3)', color: '#52c41a', borderRadius: 3 }}>自定义</Tag>}
                <div style={{ ...iconTileStyle, marginBottom: 8 }}>
                  <img src={getSvgUrl(icon)} alt={icon.title || icon.name} style={iconImgStyle} onError={e => { (e.target as HTMLImageElement).style.display = 'none'; }} />
                </div>
                <div style={{ display: 'flex', alignItems: 'center', width: '100%', gap: 2, minHeight: 24 }}>
                  <Text style={{ flex: '1 1 auto', minWidth: 0, color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 11, textAlign: 'center', lineHeight: 1.3, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={icon.title || icon.name}>{icon.title || icon.name}</Text>
                  <div className="icon-actions" style={{ display: 'flex', flex: '0 0 auto', gap: 0, width: 0, overflow: 'hidden', opacity: 0, pointerEvents: 'none' }} onClick={e => e.stopPropagation()}>
                    <Tooltip title="编辑"><Button type="text" size="small" icon={<EditOutlined />} style={{ color: _isLight ? 'rgba(0,0,0,0.45)' : 'rgba(255,255,255,0.45)', fontSize: 12, width: 22, height: 22 }} onClick={() => handleOpenEdit(icon)} /></Tooltip>
                    <Popconfirm title="确定删除？" onConfirm={() => handleDelete(icon.id)} okText="删除" cancelText="取消">
                      <Tooltip title="删除"><Button type="text" size="small" danger icon={<DeleteOutlined />} style={{ fontSize: 12, width: 22, height: 22 }} /></Tooltip>
                    </Popconfirm>
                  </div>
                </div>
              </div>
            ))}
          </div>
          {filteredIcons.length > 0 && (
            <div style={{ textAlign: 'center', marginTop: 20 }}>
              <ListPagination current={page} total={filteredIcons.length} pageSize={pageSize} onChange={(p, s) => { setPage(p); setPageSize(s); }} />
            </div>
          )}
        </>
      )}

      {/* ════ 添加/编辑 Modal ════ */}
      <Modal
        title={editingIcon ? '编辑图标' : '添加自定义图标'}
        open={addModalVisible}
        onOk={handleSave}
        onCancel={() => setAddModalVisible(false)}
        confirmLoading={saving}
        okText={editingIcon ? '保存修改' : '添加图标'}
        width={680}
        destroyOnClose
      >
        {/* 存储方式感知条 */}
        <div style={{
          marginTop: 12,
          marginBottom: 16,
          padding: '9px 14px',
          borderRadius: 6,
          background: _isLight ? '#f9fafb' : '#18181b',
          border: _isLight ? '1px solid #e5e7eb' : '1px solid #27272a',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          fontSize: 12
        }}>
          <span style={{ color: _isLight ? '#4b5563' : '#a1a1aa' }}>当前存储绑定</span>
          {storageInfo?.is_cloud ? (
            <Tag color="cyan" style={{ margin: 0, borderRadius: 4 }}>
              云存储 ({storageInfo.provider.toUpperCase()}: {storageInfo.bucket || '已绑定'})
            </Tag>
          ) : (
            <Tag style={{ margin: 0, background: _isLight ? '#f3f4f6' : '#27272a', color: _isLight ? '#374151' : '#d1d5db', border: 'none', borderRadius: 4 }}>
              本地存储 (/data/assets)
            </Tag>
          )}
        </div>

        <Form form={form} layout="vertical">
          <Form.Item
            name="name"
            label="图标标识名"
            rules={[
              { required: true, message: '请输入图标标识名' },
              { pattern: /^[a-z0-9_-]+$/, message: '仅支持小写英文字母、数字、下划线和连字符 (例如 my-model)' }
            ]}
            extra="用于系统代码索引与模型 logo 字段（如 claude, deepseek）。保存时全站唯一。"
          >
            <Input
              placeholder="例如: my-model"
              disabled={!!editingIcon}
              onChange={e => {
                const val = e.target.value.toLowerCase().replace(/\s+/g, '-');
                form.setFieldsValue({ name: val });
              }}
            />
          </Form.Item>
          <Form.Item
            name="title"
            label="显示名称"
            extra="前台与列表展示给用户的友好名称，支持中文（例如：智谱清言、豆包、自建大模型）。"
          >
            <Input placeholder="例如: My Model" />
          </Form.Item>
          <Form.Item name="category" label="分类" initialValue="自定义">
            <Select options={[{ label: 'AI品牌', value: 'AI品牌' }, { label: '自定义', value: '自定义' }]} />
          </Form.Item>
          <Form.Item name="tags" label="标签" extra="可选，按回车添加检索标签。">
            <Select mode="tags" placeholder="例如: 国产, 视觉, 开源" />
          </Form.Item>
          <Form.Item label="SVG 内容" required={!editingIcon} extra="打开编辑会载入当前 SVG。可改尺寸、底色和图标填充，也可直接改源码。底色会替换铺满画布的色块，填充会写进其余图形。">
            {svgLoading && <div style={{ textAlign: 'center', padding: 12 }}><Spin size="small" /></div>}
            {svgPreview.includes('<svg') && (
              <div style={{ marginBottom: 12 }}>
                <div style={{ display: 'flex', gap: 16, marginBottom: 12 }}>
                  {([
                    ['浅色底', '#f5f5f5', '#141414'],
                    ['深色底', '#141414', '#ffffff'],
                  ] as const).map(([label, background, ink]) => (
                    <div key={label} style={{ textAlign: 'center' }}>
                      <div style={{
                        width: 96, height: 96, borderRadius: 8, background, color: ink,
                        display: 'flex', alignItems: 'center', justifyContent: 'center',
                        border: '1px solid rgba(128,128,128,0.28)',
                      }}>
                        <div dangerouslySetInnerHTML={{ __html: fitSvgPreview(svgPreview) }} />
                      </div>
                      <Text style={{ fontSize: 11, color: _isLight ? '#6b7280' : '#a1a1aa' }}>{label}</Text>
                    </div>
                  ))}
                </div>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  <Space wrap>
                    <Text style={{ fontSize: 12 }}>尺寸</Text>
                    <InputNumber min={1} max={512} value={svgWidth} onChange={v => typeof v === 'number' && patchSvg({ width: v })} />
                    <Text style={{ fontSize: 12 }}>×</Text>
                    <InputNumber min={1} max={512} value={svgHeight} onChange={v => typeof v === 'number' && patchSvg({ height: v })} />
                  </Space>
                  <Space wrap>
                    <Text style={{ fontSize: 12 }}>底色</Text>
                    {([
                      ['无', null],
                      ['白色', '#ffffff'],
                      ['浅灰', '#f5f5f5'],
                      ['深色', '#141414'],
                    ] as const).map(([label, color]) => (
                      <Button key={label} size="small" type={(svgBg || '').toLowerCase() === (color || '').toLowerCase() ? 'primary' : 'default'} onClick={() => patchSvg({ background: color })}>{label}</Button>
                    ))}
                    <ColorPicker size="small" disabledAlpha allowClear value={svgBg || undefined} onChange={color => patchSvg({ background: color.cleared ? null : color.toHexString() })} />
                  </Space>
                  <Space wrap>
                    <Text style={{ fontSize: 12 }}>填充</Text>
                    <Button size="small" type={svgFill ? 'default' : 'primary'} onClick={() => patchSvg({ iconColor: null })}>保持原色</Button>
                    <Button size="small" type={svgFill?.toLowerCase() === '#ffffff' ? 'primary' : 'default'} onClick={() => patchSvg({ iconColor: '#ffffff' })}>改为白色</Button>
                    <Button size="small" type={svgFill?.toLowerCase() === '#1677ff' ? 'primary' : 'default'} onClick={() => patchSvg({ iconColor: '#1677ff' })}>改为蓝色</Button>
                    <ColorPicker size="small" disabledAlpha allowClear value={svgFill || undefined} onChange={color => patchSvg({ iconColor: color.cleared ? null : color.toHexString() })} />
                  </Space>
                  {svgOriginal && svgPreview.trim() !== svgOriginal.trim() && (
                    <Button size="small" onClick={() => absorbSvg(svgOriginal, true)}>恢复原始 SVG</Button>
                  )}
                </div>
              </div>
            )}
            <Dragger
              accept=".svg"
              showUploadList={false}
              beforeUpload={handleSvgUpload as any}
              style={{
                background: _isLight ? '#fafafa' : '#18181b',
                borderColor: _isLight ? '#e5e7eb' : '#27272a',
                borderRadius: 8,
                padding: '12px 0'
              }}
            >
              <p style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', margin: '4px 0' }}>
                <InboxOutlined style={{ fontSize: 28 }} />
              </p>
              <p style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13, margin: '0 0 4px' }}>
                点击或拖拽 SVG 文件到此处替换
              </p>
            </Dragger>
            <div style={{ marginTop: 8 }}>
              <TextArea
                rows={6}
                placeholder="<svg viewBox='0 0 24 24' ...>...</svg>"
                value={svgPreview}
                onChange={e => {
                  const v = e.target.value;
                  svgBaseRef.current = v;
                  const meta = readSvgMeta(v);
                  editsRef.current = { width: meta.width, height: meta.height, background: meta.background, iconColor: null };
                  setSvgPreview(v);
                  setSvgWidth(meta.width);
                  setSvgHeight(meta.height);
                  setSvgBg(meta.background);
                  setSvgFill(null);
                }}
                style={{
                  background: _isLight ? '#fafafa' : '#18181b',
                  borderColor: _isLight ? '#e5e7eb' : '#27272a',
                  fontFamily: 'monospace',
                  fontSize: 12
                }}
              />
            </div>
          </Form.Item>
        </Form>
      </Modal>

      {/* ════ 预览 Modal ════ */}
      <Modal title={previewIcon ? previewIcon.title || previewIcon.name : ''} open={!!previewIcon} onCancel={() => setPreviewIcon(null)} footer={null} width={400}>
        {previewIcon && (
          <div style={{ textAlign: 'center', padding: '20px 0' }}>
            <div style={{ ...iconTileStyle, margin: '0 auto 16px' }}>
              <img src={getSvgUrl(previewIcon)} alt={previewIcon.name} style={iconImgStyle} onError={e => { (e.target as HTMLImageElement).style.display = 'none'; }} />
            </div>
            <div style={{ textAlign: 'left', padding: '0 12px' }}>
              {([['标识名', previewIcon.name], ['显示名称', previewIcon.title || '-'], ['分类', previewIcon.category], ['文件路径', previewIcon.file_path], ['更新时间', formatApiDateTime(previewIcon.updated_at)]] as const).map(([label, val]) => (
                <div key={label} style={{ marginBottom: 8 }}>
                  <Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>{label}</Text>
                  <Text style={{ color: val === previewIcon.file_path ? 'rgba(255,255,255,0.5)' : '#fff', display: 'block', fontSize: val === previewIcon.file_path ? 12 : 14, wordBreak: 'break-all' }}>{val}</Text>
                </div>
              ))}
              <div style={{ marginBottom: 8 }}><Text style={{ color: _isLight ? 'rgba(0,0,0,0.4)' : 'rgba(255,255,255,0.4)', fontSize: 12 }}>来源</Text><Tag color={previewIcon.source === 'custom' ? 'green' : 'blue'} style={{ marginLeft: 8 }}>{previewIcon.source === 'custom' ? '手动添加' : 'Lobe Icons'}</Tag></div>
            </div>
          </div>
        )}
      </Modal>

      {/* ════ 同步历史日志 Modal ════ */}
      <Modal title="同步历史日志" open={logModalVisible} onCancel={() => setLogModalVisible(false)} footer={null} width={600}>
        <PluginLogRetentionCard pluginName="site_icons" title="图标同步日志保留天数" style={{ marginBottom: 12 }} />
        {logsLoading ? <div style={{ textAlign: 'center', padding: 40 }}><Spin /></div>
         : syncLogList.length === 0 ? <Empty description="暂无同步记录" />
         : <div style={{ maxHeight: 400, overflow: 'auto' }}>
            {syncLogList.map(log => (
              <div key={log.id} style={{ background: _isLight ? '#fff' : '#141414', borderRadius: 6, border: _isLight ? '1px solid rgba(0,0,0,0.06)' : '1px solid rgba(255,255,255,0.06)', padding: '12px 16px', marginBottom: 8 }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
                  <Tag color={log.status === 'success' ? 'success' : log.status === 'partial' ? 'warning' : 'error'}>{log.status === 'success' ? '成功' : log.status === 'partial' ? '部分成功' : '失败'}</Tag>
                  <Text style={{ color: _isLight ? 'rgba(0,0,0,0.35)' : 'rgba(255,255,255,0.35)', fontSize: 12 }}>{formatApiDateTime(log.created_at)}</Text>
                </div>
                <Text style={{ color: _isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)', fontSize: 13 }}>同步 {log.total_synced} 个，库中共 {log.total_new} 个</Text>
                {log.error_message && <div style={{ marginTop: 4 }}><Text style={{ color: 'rgba(255,77,79,0.8)', fontSize: 12, wordBreak: 'break-all' }}>{log.error_message.substring(0, 200)}</Text></div>}
              </div>
            ))}
          </div>}
      </Modal>
    </div>
  );
};

export default SiteIconsManager;
