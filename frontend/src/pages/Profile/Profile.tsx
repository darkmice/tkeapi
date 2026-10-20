/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState, useCallback, useRef } from 'react';
import { Card, Typography, Avatar, Space, List, Button, Modal, Form, Input, message, Popconfirm, Select, Grid, Tag, Spin, Alert, DatePicker } from 'antd';
import { UserOutlined, CameraOutlined, LockOutlined, MailOutlined, MobileOutlined, WechatOutlined, GoogleOutlined, SafetyOutlined, IdcardOutlined, EyeOutlined, RedoOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';
import { useTranslation } from 'react-i18next';
import { useSearchParams, useNavigate } from 'react-router-dom';
import { Settings } from 'lucide-react';
import request from '../../utils/request';
import type { User, UserKyc, UserKycStatus, UserKycType } from '../../types';
import useAuthStore from '../../store/auth';
import useSettingsStore from '../../store/settings';
import { useThemeStore } from '../../store/theme';
import WechatQR from '../../components/WechatQR';
import UserKycFormFields, {
  kycToFormValues,
  bothKycToFormValues,
  formValuesToKycPayload,
} from '../../components/UserKycFormFields';
import UserKycDetailViewer from '../../components/UserKycDetailViewer';
import AvatarCropModal from '../../components/AvatarCropModal';
import PageHeader from '../../components/PageHeader';
import { maskDocNumber } from '../../utils/maskDocNumber';

const { Title, Text } = Typography;

function birthYmPayload(value: unknown): string {
  if (dayjs.isDayjs(value)) {
    return value.isValid() ? value.format('YYYY-MM') : '';
  }
  if (typeof value === 'string') return value.trim();
  return '';
}

function formatBirthYm(value: string | undefined, isEn: boolean) {
  if (!value) return value;
  const matched = /^(\d{4})-(\d{2})$/.exec(value);
  if (!matched || isEn) return value;
  return `${matched[1]}年${matched[2]}月`;
}

/** 姓在前：中文、日文、韩文、越南文。其余语言名在前、姓在后。 */
const SURNAME_FIRST_LANG = /^(zh|ja|ko|vi)(-|$)/i;

function isCjkNamePart(value: string) {
  return /^[\u3400-\u9fff\uf900-\ufaff\u3040-\u30ff\uac00-\ud7af·・\s]+$/.test(value);
}

function formatProfileName(firstName?: string | null, lastName?: string | null, language?: string) {
  const given = (firstName || '').trim();
  const family = (lastName || '').trim();
  if (!given && !family) return undefined;
  const surnameFirst = SURNAME_FIRST_LANG.test((language || 'zh').replace('_', '-'));
  const parts = (surnameFirst ? [family, given] : [given, family]).filter(Boolean);
  if (parts.length < 2) return parts[0];
  const tight = surnameFirst && parts.every(isCjkNamePart);
  return parts.join(tight ? '' : ' ');
}

const timezoneOptions = (() => {
  const timezones = Intl.supportedValuesOf ? Intl.supportedValuesOf('timeZone') : [
    'Asia/Shanghai', 'Asia/Tokyo', 'America/New_York', 'Europe/London'
  ];
  
  const grouped: Record<string, { value: string, label: string }[]> = {};
  
  timezones.forEach(tz => {
    const parts = tz.split('/');
    if (parts.length >= 2) {
      const group = parts[0];
      const city = parts.slice(1).join('/').replace(/_/g, ' ');
      
      const date = new Date();
      const str = date.toLocaleString('en-US', { timeZone: tz, timeZoneName: 'shortOffset' });
      const match = str.match(/(GMT|UTC)([+-]\d{1,2}(:\d{2})?)/);
      let offset = '';
      if (match) {
        offset = ` (UTC${match[2]})`;
      } else if (str.includes('GMT') || str.includes('UTC')) {
        offset = ' (UTC+0)';
      }
      
      if (!grouped[group]) grouped[group] = [];
      grouped[group].push({ value: tz, label: `${tz.replace(/_/g, ' ')}${offset}` });
    }
  });
  
  return Object.entries(grouped)
    .map(([group, options]) => ({
      label: group,
      options: options.sort((a, b) => a.label.localeCompare(b.label))
    }))
    .sort((a, b) => a.label.localeCompare(b.label));
})();

const formatTimezoneDisplay = (tz: string) => {
  if (!tz) return '';
  const displayTz = tz.replace(/_/g, ' ');
  try {
    const date = new Date();
    const str = date.toLocaleString('en-US', { timeZone: tz, timeZoneName: 'shortOffset' });
    const match = str.match(/(GMT|UTC)([+-]\d{1,2}(:\d{2})?)/);
    if (match) return `${displayTz} (UTC${match[2]})`;
    if (str.includes('GMT') || str.includes('UTC')) return `${displayTz} (UTC+0)`;
  } catch (e) {}
  return displayTz;
};



const Profile: React.FC = () => {
  const { t, i18n } = useTranslation();
  const isEn = i18n.language === 'en';
  const navigate = useNavigate();
  const [profile, setProfile] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);
  const [isModalVisible, setIsModalVisible] = useState(false);
  const [modalType, setModalType] = useState<string>('nickname');
  const [form] = Form.useForm();
  const [kycForm] = Form.useForm();
  const { setUser, logout } = useAuthStore();
  const { settings, fetchSettings } = useSettingsStore();
  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';
  const cardBg = isLight ? '#fff' : '#141414';
  const cardBorder = isLight ? '1px solid #e8e8e8' : '1px solid #303030';
  const listBorder = isLight ? '1px solid #f0f0f0' : '1px solid #303030';
  const mainText = isLight ? '#09090b' : '#fafafa';
  const subText = isLight ? '#71717a' : '#a1a1aa';
  const avatarBg = isLight ? '#e8e8e8' : '#303030';
  const avatarBorder = isLight ? '2px solid #d9d9d9' : '2px solid #505050';
  const [countdowns, setCountdowns] = useState<Record<string, number>>({});
  const [avatarFile, setAvatarFile] = useState<File | null>(null);
  const avatarInputRef = useRef<HTMLInputElement | null>(null);
  const [searchParams, setSearchParams] = useSearchParams();
  const { useBreakpoint } = Grid;
  const screens = useBreakpoint();

  // 微信换绑步骤：'verify'=验证旧微信, 'bind'=绑定新微信
  const [wechatBindStep, setWechatBindStep] = useState<'verify' | 'bind'>('verify');
  // 每次切换步骤时重新生成 key，确保二维码刷新
  const [wechatQRKey, setWechatQRKey] = useState(() => Date.now());
  // 服务端 HMAC 签发的绑定/验证 state
  const [wechatBindState, setWechatBindState] = useState('');
  // 实名认证 KYC 列表与状态（支持个人与企业 2 条记录独立共存，互不覆盖）
  const [kycList, setKycList] = useState<UserKyc[]>([]);
  const [activeKycTab, setActiveKycTab] = useState<'personal' | 'enterprise'>('personal');
  const [modalKycType, setModalKycType] = useState<'personal' | 'enterprise'>('personal');
  const [kycLoading, setKycLoading] = useState(false);
  const [kycDetailModalOpen, setKycDetailModalOpen] = useState(false);
  const [kycFormModalOpen, setKycFormModalOpen] = useState(false);
  const [kycSaving, setKycSaving] = useState(false);
  const [isReKycMode, setIsReKycMode] = useState(false);
  const kycEnabled = settings?.registration?.enable_user_kyc === true;

  const personalKyc = kycList.find(k => k.kyc_type === 'personal') || null;
  const enterpriseKyc = kycList.find(k => k.kyc_type === 'enterprise') || null;

  const startCountdown = (key: string) => {
    setCountdowns(prev => ({ ...prev, [key]: 60 }));
  };

  const login = settings?.login;

  const fetchProfile = useCallback(async () => {
    setLoading(true);
    try {
      const resp = await (request.get('/user/profile') as unknown as Promise<User>);
      setProfile(resp);
      setUser(resp);
    } catch (e) { console.error(e); }
    finally { setLoading(false); }
  }, [setUser]);

  useEffect(() => {
    fetchProfile();
    fetchSettings(true);
  }, [fetchProfile, fetchSettings]);

  const handleAvatarPicked = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      message.error(t('profile.avatar_invalid_type', '头像只支持图片格式'));
      return;
    }
    if (file.size > 50 * 1024 * 1024) {
      message.error(t('profile.avatar_too_large', '图片不能超过 50MB'));
      return;
    }
    setAvatarFile(file);
  };

  const handleAvatarCropped = async (blob: Blob) => {
    try {
      const formData = new FormData();
      formData.append('file', blob, 'avatar.jpg');
      const res = await (request.post('/user/avatar', formData, {
        headers: { 'Content-Type': 'multipart/form-data' },
      }) as unknown as Promise<{ avatar: string }>);
      if (profile) {
        const next = { ...profile, avatar: res.avatar };
        setProfile(next);
        setUser(next);
      }
      setAvatarFile(null);
      message.success(t('profile.avatar_updated', '头像已更新'));
    } catch (err) {
      const e = err as { response?: { data?: { error?: { message?: string } } }; message?: string };
      message.error(e?.response?.data?.error?.message || e?.message || t('profile.avatar_upload_failed', '头像上传失败'));
    }
  };

  const fetchKyc = useCallback(async () => {
    setKycLoading(true);
    try {
      const resp = await (request.get('/user/kyc/list') as unknown as Promise<UserKyc[]>);
      const list = Array.isArray(resp) ? resp : [];
      setKycList(list);
      // 严格以默认实名主体（is_default === true）为准进行同步！
      const defaultItem = list.find(k => k.is_default) || list[0];
      const targetType = (defaultItem?.kyc_type as 'personal' | 'enterprise') || 'personal';
      setActiveKycTab(targetType);
    } catch (e) {
      console.error(e);
      setKycList([]);
    } finally {
      setKycLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchKyc();
  }, [fetchKyc]);

  const openKycDetail = (type?: 'personal' | 'enterprise') => {
    const defaultItem = kycList.find(k => k.is_default) || kycList[0];
    const targetType = type || (defaultItem?.kyc_type as 'personal' | 'enterprise') || activeKycTab;
    setModalKycType(targetType);
    setKycDetailModalOpen(true);
  };

  const openKycForm = (type?: 'personal' | 'enterprise', reKyc = false) => {
    const defaultItem = kycList.find(k => k.is_default) || kycList[0];
    const targetType = type || (defaultItem?.kyc_type as 'personal' | 'enterprise') || activeKycTab;
    setModalKycType(targetType);
    setIsReKycMode(reKyc);
    kycForm.resetFields();
    kycForm.setFieldsValue(bothKycToFormValues(targetType, personalKyc, enterpriseKyc));
    setKycFormModalOpen(true);
  };

  const handleModalTypeChange = (newType: UserKycType) => {
    setModalKycType(newType);
    const target = newType === 'enterprise' ? enterpriseKyc : personalKyc;
    kycForm.setFieldsValue({
      validity_type: (target?.validity_type as any) || 'long_term',
      expire_at: target?.expire_at ? dayjs(target.expire_at) : null,
      is_default: true,
    });
  };

  const handleSubmitKyc = async () => {
    try {
      const validatedValues = await kycForm.validateFields();
      setKycSaving(true);
      const allFormValues = { ...kycForm.getFieldsValue(true), ...validatedValues };
      // 用户端选择编辑并提交的主体，自动成为生效主体
      allFormValues.is_default = true;
      const payload = formValuesToKycPayload(allFormValues, false);
      await request.put('/user/kyc', payload);
      message.success(
        kycEnabled
          ? (isEn ? 'Information saved, waiting for review' : (isReKycMode ? '重新实名申请已提交，等待管理员审核' : '实名认证资料已提交，等待审核'))
          : t('profile.edit_success', '实名信息已成功保存')
      );
      setKycFormModalOpen(false);
      setIsReKycMode(false);
      const savedType = (allFormValues.kyc_type as 'personal' | 'enterprise') || modalKycType;
      setActiveKycTab(savedType);
      await fetchKyc();
    } catch (e: any) {
      if (e?.errorFields) return;
      message.error(e?.message || '保存实名信息失败');
      console.error(e);
    } finally {
      setKycSaving(false);
    }
  };


  // 打开微信绑定弹窗或切换步骤时，向后端获取 HMAC state
  useEffect(() => {
    if (!isModalVisible || modalType !== 'bind_wechat') return;
    let cancelled = false;
    setWechatBindState('');
    request
      .get('/user/bind/oauth-state', {
        params: { 
          provider: 'wechat', 
          action: wechatBindStep,
          origin: window.location.origin,
        },
        skipErrorHandler: true,
      } as any)
      .then((res: any) => {
        if (!cancelled && res?.state) setWechatBindState(res.state);
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [isModalVisible, modalType, wechatBindStep, wechatQRKey, settings?.wechat_oauth_callback_domain]);

  // ── 监听微信回调 URL 参数 ──────────────────────────────────
  useEffect(() => {
    const action = searchParams.get('wechat_action');
    if (action) {
      // 消费掉参数，避免刷新重复触发
      searchParams.delete('wechat_action');
      setSearchParams(searchParams, { replace: true });

      switch (action) {
        case 'verified':
          message.success(t('profile.wechat_verify_success', '身份验证通过，请用新微信扫码绑定'));
          setWechatBindStep('bind');
          setWechatQRKey(Date.now());
          setModalType('bind_wechat');
          setIsModalVisible(true);
          break;
        case 'verify_failed':
          message.error(t('profile.wechat_verify_failed', '验证失败：扫码微信与当前绑定的微信不一致'));
          break;
        case 'bindok':
          message.success(t('profile.wechat_bind_success', '微信绑定成功'));
          setIsModalVisible(false);
          fetchProfile();
          break;
        case 'bindconflict':
          message.error(t('profile.wechat_bind_conflict', '该微信绑定账号数已达上限'));
          break;
      }
    }

    const googleAction = searchParams.get('google_action');
    if (googleAction) {
      searchParams.delete('google_action');
      setSearchParams(searchParams, { replace: true });
      switch (googleAction) {
        case 'verified':
          Modal.confirm({
            title: t('auth.identity_verified') || '身份验证通过',
            content: t('auth.google_bind_confirm_text') || '原谷歌账号身份验证通过，是否立即前往绑定新谷歌账号？',
            okText: t('auth.go_to_bind') || '前往绑定',
            cancelText: t('common.cancel') || '取消',
            onOk: () => {
              window.location.href = '/api/v1/user/bind/google?action=bind';
            }
          });
          break;
        case 'verify_failed':
          message.error(t('auth.google_verify_failed') || '验证失败：授权的谷歌账号与当前绑定的谷歌账号不一致');
          break;
        case 'bindok':
          message.success(t('auth.google_bind_success') || '谷歌账号绑定成功');
          fetchProfile();
          break;
        case 'bindconflict':
          message.error(t('auth.google_bind_conflict') || '此谷歌账号已绑定其他账号');
          break;
      }
    }
  }, [searchParams, setSearchParams, fetchProfile, t]);

  useEffect(() => {
    const activeKeys = Object.keys(countdowns).filter(k => countdowns[k] > 0);
    if (activeKeys.length === 0) return;
    const timer = setInterval(() => {
      setCountdowns(prev => {
        const next = { ...prev };
        let changed = false;
        Object.keys(next).forEach(k => { if (next[k] > 0) { next[k] -= 1; changed = true; } });
        return changed ? next : prev;
      });
    }, 1000);
    return () => clearInterval(timer);
  }, [countdowns]);

  const handleAction = (type: string) => {
    const modal = type === 'first_name' || type === 'last_name' ? 'name' : type;
    setModalType(modal);
    form.resetFields();
    if (modal === 'bind_wechat') {
      // 已绑定微信 → 换绑模式（先验证）；未绑定 → 直接绑定
      setWechatBindStep(profile?.wechat_id ? 'verify' : 'bind');
      setWechatQRKey(Date.now());
    } else if (modal === 'timezone') {
      form.setFieldsValue({ timezone: profile?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai' });
    } else if (modal === 'nickname') {
      form.setFieldsValue({ nickname: profile?.nickname || '' });
    } else if (modal === 'name') {
      form.setFieldsValue({ first_name: profile?.first_name || '', last_name: profile?.last_name || '' });
    } else if (modal === 'birth_ym') {
      form.setFieldsValue({ birth_ym: profile?.birth_ym ? dayjs(profile.birth_ym) : null });
    }
    setIsModalVisible(true);
  };

  // ── 监听 URL 快速绑定跳转参数（例如弹窗引导的 ?bind=mobile 或 ?bind=email） ────
  useEffect(() => {
    const bindTarget = searchParams.get('bind');
    if (!bindTarget) return;

    searchParams.delete('bind');
    setSearchParams(searchParams, { replace: true });

    if (bindTarget === 'mobile') {
      handleAction('bind_mobile');
    } else if (bindTarget === 'email') {
      handleAction('bind_email');
    }
  }, [searchParams, setSearchParams]);

  const handleUpdate = async (values: any) => {
    try {
      const payload = { ...values };
      if (modalType === 'birth_ym') {
        payload.birth_ym = birthYmPayload(payload.birth_ym);
      }
      await request.put('/user/profile', payload);
      if (payload.password) {
        message.success(t('auth.reset_password_success', '密码修改成功，请重新登录'));
        setIsModalVisible(false);
        logout();
        navigate('/login');
        return;
      }
      message.success(t('profile.edit_success'));
      setIsModalVisible(false);
      fetchProfile();
    } catch (e) { console.error(e); }
  };

  const handleBindMobile = async (values: any) => {
    try {
      await (request.post('/user/bind/mobile', values) as any);
      message.success(t('profile.edit_success'));
      setIsModalVisible(false);
      fetchProfile();
    } catch (e) { console.error(e); }
  };

  const handleBindEmail = async (values: any) => {
    try {
      await (request.post('/user/bind/email', values) as any);
      message.success(t('profile.edit_success'));
      setIsModalVisible(false);
      fetchProfile();
    } catch (e) { console.error(e); }
  };

  const handleUnbind = async (type: string, password: string) => {
    try {
      await (request.post(`/user/unbind/${type}`, { password }) as any);
      message.success(t('profile.unbind_success'));
      fetchProfile();
    } catch (e) { console.error(e); }
  };

  const sendCode = async (target: string, type: 'email' | 'sms', purpose: string, timerKey: string) => {
    if (countdowns[timerKey] > 0) return;
    try {
      if (type === 'email') {
        await request.post('/auth/send-code', { email: target, purpose });
        message.success(t('auth.code_sent'));
      } else {
        await request.post('/auth/send-sms-code', { mobile: target, purpose });
        message.success(t('auth.sms_code_sent'));
      }
      startCountdown(timerKey);
    } catch (e) { console.error(e); }
  };

  // 构建安全设置项
  const buildSecurityItems = () => {
    const items: { key: string; label: string; value: string; action: string; icon: React.ReactNode; handler: () => void }[] = [];
    items.push({ key: 'password', label: t('profile.password'), value: '********', action: t('profile.edit'), icon: <LockOutlined />, handler: () => handleAction('password') });
    if (login?.enable_mobile_login) {
      const hasMobile = !!profile?.mobile;
      items.push({ key: 'mobile', label: t('profile.mobile'), value: profile?.mobile || t('profile.not_bound'), action: hasMobile ? t('profile.rebind') : t('profile.bind'), icon: <MobileOutlined />, handler: () => handleAction('bind_mobile') });
    }
    if (login?.enable_email_login) {
      const hasEmail = !!profile?.email && !profile.email.endsWith('@tokensbyte.local');
      items.push({ key: 'email', label: t('profile.email'), value: hasEmail ? profile!.email : t('profile.not_bound'), action: hasEmail ? t('profile.rebind') : t('profile.bind'), icon: <MailOutlined />, handler: () => handleAction('bind_email') });
    }
    if (login?.enable_wechat_login) {
      items.push({ 
        key: 'wechat', 
        label: t('profile.wechat'), 
        value: profile?.wechat_id ? `${t('profile.bound')} ${profile.wechat_name ? `(${profile.wechat_name})` : ''}` : t('profile.not_bound'), 
        action: profile?.wechat_id ? t('profile.rebind') : t('profile.bind'), 
        icon: <WechatOutlined />, 
        handler: () => handleAction('bind_wechat') 
      });
    }
    if (login?.enable_google_login) {
      items.push({ 
        key: 'google', 
        label: t('profile.google'), 
        value: profile?.google_id ? `${t('profile.bound')} ${profile.google_name ? `(${profile.google_name})` : ''}` : t('profile.not_bound'), 
        action: profile?.google_id ? t('profile.rebind') : t('profile.bind'), 
        icon: <GoogleOutlined />, 
        handler: () => { 
          if (profile?.google_id) {
            window.location.href = '/api/v1/user/bind/google?action=verify';
          } else {
            window.location.href = '/api/v1/user/bind/google?action=bind';
          }
        } 
      });
    }
    return items;
  };

  const securityItems = buildSecurityItems();
  const basicInfoItems = [
    { key: 'username', label: t('profile.account'), value: profile?.username },
    { key: 'nickname', label: t('profile.nickname'), value: profile?.nickname },
    {
      key: 'name',
      label: t('profile.name', '姓名'),
      value: formatProfileName(profile?.first_name, profile?.last_name, i18n.language),
    },
    { key: 'birth_ym', label: t('profile.birth_ym', '出生年月'), value: formatBirthYm(profile?.birth_ym, isEn) },
    { key: 'timezone', label: t('profile.timezone', '时区'), value: formatTimezoneDisplay(profile?.timezone || Intl.DateTimeFormat().resolvedOptions().timeZone || 'Asia/Shanghai') },
  ];

  // Modal 内容
  const renderModalContent = () => {
    switch (modalType) {
      case 'nickname':
        return (
          <Form.Item
            name="nickname"
            label={t('profile.nickname')}
            rules={[
              { required: true, message: t('profile.nickname_required', '昵称不能为空') },
              { max: 24, message: t('profile.nickname_max_length', '昵称长度最多不能超过 24 个字符') }
            ]}
          >
            <Input placeholder={t('profile.nickname')} />
          </Form.Item>
        );
      case 'name':
        return (
          <>
            <Form.Item
              name="first_name"
              label={t('profile.first_name', '名')}
              rules={[{ max: 32, message: t('profile.first_name_max', '名长度最多不能超过 32 个字符') }]}
            >
              <Input placeholder={t('profile.first_name', '名')} allowClear />
            </Form.Item>
            <Form.Item
              name="last_name"
              label={t('profile.last_name', '姓氏')}
              rules={[{ max: 32, message: t('profile.last_name_max', '姓氏长度最多不能超过 32 个字符') }]}
            >
              <Input placeholder={t('profile.last_name', '姓氏')} allowClear />
            </Form.Item>
          </>
        );
      case 'birth_ym':
        return (
          <Form.Item name="birth_ym" label={t('profile.birth_ym', '出生年月')}>
            <DatePicker
              picker="month"
              style={{ width: '100%' }}
              placeholder={t('profile.birth_ym_placeholder', '选择出生年月')}
              disabledDate={(current) => !!current && (current.isAfter(dayjs(), 'month') || current.year() < 1900)}
            />
          </Form.Item>
        );
      case 'timezone':
        return (
          <Form.Item name="timezone" label={t('profile.timezone', '时区')} rules={[{ required: true }]}>
            <Select 
              showSearch 
              placeholder={t('profile.select_timezone', '请选择时区')} 
              options={timezoneOptions} 
              filterOption={(input, option: any) =>
                (option?.label as string ?? '').toLowerCase().includes(input.toLowerCase()) ||
                (option?.value as string ?? '').toLowerCase().includes(input.toLowerCase())
              }
            />
          </Form.Item>
        );
      case 'password':
        return (
          <>
            <Form.Item name="old_password" label={t('profile.old_password', '原密码')} rules={[{ required: true, min: 6, message: t('profile.verify_current_password', '请验证目前的登录密码') }]}>
              <Input.Password placeholder={t('profile.enter_old_password', '请输入原密码以验证身份')} />
            </Form.Item>
            <Form.Item name="password" label={t('profile.password')} rules={[{ required: true, min: 6 }]}>
              <Input.Password placeholder={t('profile.enter_new_password', '请输入新密码')} />
            </Form.Item>
            <Form.Item name="confirm" label={t('login.confirm_password')} dependencies={['password']}
              rules={[{ required: true, message: t('auth.confirm_password_required') }, ({ getFieldValue }) => ({
                validator(_, value) { return !value || getFieldValue('password') === value ? Promise.resolve() : Promise.reject(new Error(t('auth.passwords_not_match'))); },
              })]}>
              <Input.Password placeholder={t('auth.confirm_password_placeholder')} />
            </Form.Item>
          </>
        );
      case 'bind_mobile':
        return (
          <Form form={form} layout="vertical" onFinish={handleBindMobile}>
            {profile?.mobile && (
              <>
                <Text type="secondary" style={{ display: 'block', marginBottom: 8 }}>{t('profile.current_mobile')}: {profile.mobile}</Text>
                <Form.Item name="old_code" label={t('profile.old_mobile_code')} rules={[{ required: true, message: t('auth.code_required') }]}>
                  <Input prefix={<SafetyOutlined />} placeholder={t('profile.enter_old_code')}
                    suffix={<Button type="link" size="small" disabled={countdowns['old_mobile'] > 0}
                      onClick={() => sendCode(profile.mobile!, 'sms', 'bind_mobile', 'old_mobile')}>
                      {countdowns['old_mobile'] > 0 ? `${countdowns['old_mobile']}s` : t('auth.send_code')}
                    </Button>} />
                </Form.Item>
              </>
            )}
            <Form.Item name="mobile" label={t('profile.bind_mobile_title')} rules={[{ required: true, message: t('auth.mobile_required') }]}>
              <Input prefix={<MobileOutlined />} placeholder={t('profile.new_mobile_placeholder')} />
            </Form.Item>
            <Form.Item name="code" label={t('profile.new_mobile_code')} rules={[{ required: true, message: t('auth.code_required') }]}>
              <Input prefix={<SafetyOutlined />} placeholder={t('profile.enter_new_code')}
                suffix={<Button type="link" size="small" disabled={countdowns['new_mobile'] > 0}
                  onClick={() => { const v = form.getFieldValue('mobile'); if (v) sendCode(v, 'sms', 'bind_mobile', 'new_mobile'); else message.warning(t('auth.mobile_required')); }}>
                  {countdowns['new_mobile'] > 0 ? `${countdowns['new_mobile']}s` : t('auth.send_code')}
                </Button>} />
            </Form.Item>
          </Form>
        );
      case 'bind_email':
        return (
          <Form form={form} layout="vertical" onFinish={handleBindEmail}>
            {profile?.email && !profile.email.endsWith('@tokensbyte.local') && (
              <>
                <Text type="secondary" style={{ display: 'block', marginBottom: 8 }}>{t('profile.current_email')}: {profile.email}</Text>
                <Form.Item name="old_code" label={t('profile.old_email_code')} rules={[{ required: true, message: t('auth.code_required') }]}>
                  <Input prefix={<SafetyOutlined />} placeholder={t('profile.enter_old_code')}
                    suffix={<Button type="link" size="small" disabled={countdowns['old_email'] > 0}
                      onClick={() => sendCode(profile.email, 'email', 'bind_email', 'old_email')}>
                      {countdowns['old_email'] > 0 ? `${countdowns['old_email']}s` : t('auth.send_code')}
                    </Button>} />
                </Form.Item>
              </>
            )}
            <Form.Item name="email" label={t('profile.bind_email_title')} rules={[{ required: true, type: 'email', message: t('auth.email_required') }]}>
              <Input prefix={<MailOutlined />} placeholder={t('profile.new_email_placeholder')} />
            </Form.Item>
            <Form.Item name="code" label={t('profile.new_email_code')} rules={[{ required: true, message: t('auth.code_required') }]}>
              <Input prefix={<SafetyOutlined />} placeholder={t('profile.enter_new_code')}
                suffix={<Button type="link" size="small" disabled={countdowns['new_email'] > 0}
                  onClick={() => { const v = form.getFieldValue('email'); if (v) sendCode(v, 'email', 'bind_email', 'new_email'); else message.warning(t('auth.email_required')); }}>
                  {countdowns['new_email'] > 0 ? `${countdowns['new_email']}s` : t('auth.send_code')}
                </Button>} />
            </Form.Item>
          </Form>
        );
      case 'bind_wechat': {
        const appId = settings?.wechat_oauth_app_id || '';
        const wechatBaseOrigin = settings?.wechat_oauth_callback_domain || window.location.origin;
        const redirectUri = `${wechatBaseOrigin}/api/v1/user/bind/wechat/callback`;
        const isVerifyStep = wechatBindStep === 'verify';
        return (
          <div style={{ textAlign: 'center', padding: '12px 0' }}>
            {wechatBindState ? (
              <WechatQR
                key={wechatQRKey}
                appId={appId}
                redirectUri={redirectUri}
                state={wechatBindState}
              />
            ) : (
              <div style={{ padding: '40px 0', color: '#8c8c8c' }}>加载中...</div>
            )}
            <div style={{ marginTop: 8, color: '#e5e5e5', fontSize: 14 }}>
              {isVerifyStep ? t('profile.scan_current_wechat', '请用当前绑定的微信扫码验证身份') : t('profile.scan_new_wechat', '请用新微信扫码绑定')}
            </div>
            <div style={{ color: '#8c8c8c', fontSize: 12, marginTop: 4 }}>"{settings?.site?.name}"</div>
          </div>
        );
      }
      default:
        return null;
    }
  };

  const handleModalOk = () => { form.submit(); };

  const isBindModal = modalType === 'bind_mobile' || modalType === 'bind_email';

  // Modal 标题
  const getModalTitle = () => {
    if (modalType === 'bind_wechat') return wechatBindStep === 'verify' ? t('profile.verify_wechat_identity', '验证微信身份') : t('profile.bind_wechat');
    if (isBindModal) return modalType === 'bind_mobile' ? t('profile.bind_mobile_title') : t('profile.bind_email_title');
    if (modalType === 'nickname') return t('profile.modify_nickname');
    if (modalType === 'name') return t('profile.modify_name', '修改姓名');
    if (modalType === 'birth_ym') return t('profile.modify_birth_ym', '修改出生年月');
    if (modalType === 'timezone') return t('profile.timezone', '修改时区');
    if (modalType === 'password') return t('profile.modify_password');
    return '';
  };

  return (
    <div style={{ maxWidth: 1200, margin: '0 auto', padding: screens.md ? '16px 0 40px 0' : '12px 0 24px 0' }}>
      {/* Header Area */}
      <PageHeader
        title={t('menu.profile', '个人中心')}
        description={isEn ? 'Manage your personal information and security settings.' : '管理您的个人基本信息与账号安全设置。'}
        marginBottom={32}
      />
      {/* Profile Header */}
      <Card style={{ marginBottom: 24, borderRadius: 8, background: cardBg, border: cardBorder }}
        styles={{ body: { padding: '32px' } }}>
        <div style={{ display: 'flex', alignItems: 'center' }}>
          <div style={{ position: 'relative' }}>
            <Avatar size={80} src={profile?.avatar || undefined} icon={<UserOutlined style={{ color: isLight ? '#71717a' : '#a1a1aa' }} />}
              onClick={() => avatarInputRef.current?.click()}
              style={{ background: avatarBg, border: avatarBorder, cursor: 'pointer' }} />
            <Button shape="circle" size="small" icon={<CameraOutlined style={{ fontSize: 10 }} />}
              aria-label={t('profile.avatar_change', '更换头像')}
              onClick={() => avatarInputRef.current?.click()}
              style={{ position: 'absolute', bottom: 0, right: 0, background: isLight ? '#18181b' : '#fafafa', border: 'none', color: isLight ? '#fafafa' : '#18181b' }} />
            <input ref={avatarInputRef} type="file" accept="image/*" hidden onChange={handleAvatarPicked} />
          </div>
          {avatarFile && (
            <AvatarCropModal file={avatarFile} onCancel={() => setAvatarFile(null)} onConfirm={handleAvatarCropped} />
          )}
          <div style={{ marginLeft: 24 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
              <Title level={3} style={{ margin: 0, color: mainText }}>
                {profile?.nickname || profile?.username || t('profile.nickname')}
              </Title>
              {(profile?.level_name || profile?.user_group) && (
                <div style={{
                  padding: '2px 10px',
                  background: isLight ? 'rgba(0,0,0,0.04)' : 'rgba(255,255,255,0.08)',
                  borderRadius: '12px', 
                  color: isLight ? '#4b5563' : '#e5e5e5',
                  border: isLight ? '1px solid rgba(0,0,0,0.1)' : '1px solid rgba(255,255,255,0.2)',
                  fontSize: '12px', fontWeight: 600,
                }}>
                  {profile?.level_name || (profile?.user_group === 'default' ? t('profile.membership_default') : profile?.user_group)}
                </div>
              )}
            </div>
            <Text type="secondary" style={{ color: subText }}>UID: {profile?.uid}</Text>
          </div>
        </div>
      </Card>

      {/* Basic Info */}
      <Card
        style={{ marginBottom: 24, borderRadius: 8, background: cardBg, border: cardBorder, overflow: 'hidden' }}
        styles={{ body: { padding: 0 } }}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <UserOutlined style={{ color: mainText }} />
            <span style={{ fontWeight: 600, color: mainText }}>{t('profile.basic_info', '基本信息')}</span>
          </div>
        }
      >
        <List itemLayout="horizontal"
          dataSource={basicInfoItems}
          renderItem={(item, index) => (
            <List.Item 
              className="hover:bg-zinc-100/30 dark:hover:bg-zinc-800/30 transition-all duration-200"
              style={{ borderBottom: index === basicInfoItems.length - 1 ? 'none' : listBorder, padding: '16px 24px' }}
              extra={(item.key === 'nickname' || item.key === 'timezone' || item.key === 'name' || item.key === 'birth_ym') && (
                <button
                  onClick={() => handleAction(item.key)}
                  className="p-1 rounded-md border border-zinc-200 dark:border-zinc-800 text-zinc-400 dark:text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 transition-colors cursor-pointer"
                  title={t('profile.edit')}
                >
                  <Settings className="h-3.5 w-3.5" />
                </button>
              )}>
              <div style={{ width: 120 }}><Text style={{ color: subText }}>{item.label}</Text></div>
              <div style={{ flex: 1 }}><Text style={{ color: mainText }}>{item.value || t('profile.not_set')}</Text></div>
            </List.Item>
          )}
        />
      </Card>

      {/* 用户详细 */}
      {(() => {
        // 严格以默认主体为准（管理后台切换默认实名信息时，客户端完全随默认主体同步）
        const defaultItem = kycList.find(k => k.is_default) || kycList[0] || null;
        const currentType = (defaultItem?.kyc_type as 'personal' | 'enterprise') || activeKycTab;
        const isEnterprise = currentType === 'enterprise';
        const activeKyc = defaultItem || (isEnterprise ? enterpriseKyc : personalKyc);
        const hasActiveData = isEnterprise
          ? !!(enterpriseKyc?.company_name || enterpriseKyc?.company_email)
          : !!(personalKyc?.real_name || personalKyc?.personal_email);
        const activeStatus = (activeKyc?.status as UserKycStatus) || 'none';
        const isApproved = kycEnabled && activeStatus === 'approved';

        let tagColor = 'default';
        let tagLabel = isEn ? 'Not Verified' : '未实名';

        if (activeStatus === 'approved') {
          tagColor = 'success';
          tagLabel = isEn
            ? (isEnterprise ? 'Enterprise Verified' : 'Personal Verified')
            : (isEnterprise ? '企业实名' : '个人实名');
        } else if (activeStatus === 'pending') {
          tagColor = 'processing';
          tagLabel = isEn ? 'Under Review' : '实名审核中';
        } else if (activeStatus === 'rejected') {
          tagColor = 'error';
          tagLabel = isEn ? 'Rejected' : '已驳回';
        } else if (activeStatus === 'expired') {
          tagColor = 'warning';
          tagLabel = isEn ? 'Expired' : '已过期';
        }

        const items = isEnterprise ? [
          { key: 'company_name', label: isEn ? 'Company Name' : '企业名称', value: enterpriseKyc?.company_name },
          { key: 'company_doc_number', label: isEn ? 'Tax ID / Doc No.' : '纳税人识别号/税号', value: maskDocNumber(enterpriseKyc?.company_doc_number) },
          { key: 'company_email', label: isEn ? 'Contact Email' : '企业联系邮箱', value: enterpriseKyc?.company_email },
          { key: 'company_phone', label: isEn ? 'Contact Phone' : '企业联系电话', value: enterpriseKyc?.company_phone },
        ] : [
          { key: 'real_name', label: isEn ? 'Real Name' : '真实姓名', value: personalKyc?.real_name },
          { key: 'id_doc_number', label: isEn ? 'ID / Doc No.' : '证件号码', value: maskDocNumber(personalKyc?.id_doc_number) },
          { key: 'personal_email', label: isEn ? 'Contact Email' : '联系邮箱', value: personalKyc?.personal_email },
          { key: 'personal_phone', label: isEn ? 'Contact Phone' : '联系电话', value: personalKyc?.personal_phone },
        ];

        return (
          <Card
            style={{ marginBottom: 24, borderRadius: 8, background: cardBg, border: cardBorder, overflow: 'hidden' }}
            styles={{ body: { padding: 0 } }}
            title={
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                <IdcardOutlined style={{ color: mainText }} />
                <span style={{ fontWeight: 600, color: mainText }}>{isEn ? 'User Details' : '用户详细'}</span>
                {kycLoading ? (
                  <Spin size="small" />
                ) : kycEnabled ? (
                  <Tag color={tagColor}>
                    {tagLabel}
                  </Tag>
                ) : null}
              </div>
            }
            extra={
              <Button
                size="small"
                type="default"
                icon={isApproved ? <EyeOutlined style={{ fontSize: 13 }} /> : <Settings className="h-3.5 w-3.5" />}
                onClick={() => {
                  if (isApproved) {
                    openKycDetail(isEnterprise ? 'enterprise' : 'personal');
                  } else {
                    openKycForm(isEnterprise ? 'enterprise' : 'personal', false);
                  }
                }}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 5,
                  borderRadius: 6,
                  fontWeight: 500,
                  fontSize: 12,
                  background: isLight ? '#f4f4f5' : '#27272a',
                  color: isLight ? '#18181b' : '#fafafa',
                  borderColor: isLight ? '#e4e4e7' : '#3f3f46',
                }}
              >
                {isApproved
                  ? (isEn ? 'View' : '查看')
                  : (isEn ? 'Complete Info' : '完善信息')}
              </Button>
            }
          >
            {/* 驳回状态提示 */}
            {kycEnabled && activeStatus === 'rejected' && (
              <div style={{ padding: '16px 24px 0' }}>
                <Alert
                  type="error"
                  showIcon
                  message={isEn ? 'Verification Rejected' : `${isEnterprise ? '企业' : '个人'}实名审核未通过`}
                  description={activeKyc?.reject_reason || (isEn ? 'Please update and resubmit.' : '请修改后重新提交审核')}
                />
              </div>
            )}

            <List
              itemLayout="horizontal"
              loading={kycLoading}
              dataSource={items}
              renderItem={(item, index) => (
                <List.Item
                  className="hover:bg-zinc-100/30 dark:hover:bg-zinc-800/30 transition-all duration-200"
                  style={{ borderBottom: index === items.length - 1 ? 'none' : listBorder, padding: '16px 24px' }}
                >
                  <div style={{ width: 140 }}><Text style={{ color: subText }}>{item.label}</Text></div>
                  <div style={{ flex: 1 }}><Text style={{ color: mainText }}>{item.value || t('profile.not_set', '未设置')}</Text></div>
                </List.Item>
              )}
            />
          </Card>
        );
      })()}

      {/* Security */}
      <Card
        style={{ borderRadius: 8, background: cardBg, border: cardBorder, overflow: 'hidden' }}
        styles={{ body: { padding: 0 } }}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <SafetyOutlined style={{ color: mainText }} />
            <span style={{ fontWeight: 600, color: mainText }}>{t('profile.security_settings', '安全设置')}</span>
          </div>
        }
      >
        <List itemLayout="horizontal" dataSource={securityItems}
          renderItem={(item, index) => (
            <List.Item 
              className="hover:bg-zinc-100/30 dark:hover:bg-zinc-800/30 transition-all duration-200"
              style={{ borderBottom: index === securityItems.length - 1 ? 'none' : listBorder, padding: '16px 24px' }}
              extra={
                <Space>
                  <button
                    onClick={item.handler}
                    className="p-1 rounded-md border border-zinc-200 dark:border-zinc-800 text-zinc-400 dark:text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-900 transition-colors cursor-pointer"
                    title={item.action}
                  >
                    <Settings className="h-3.5 w-3.5" />
                  </button>
                  {(item.key === 'wechat' && profile?.wechat_id) || (item.key === 'google' && profile?.google_id) ? (
                    <Popconfirm title={t('profile.unbind_confirm')}
                      description={<Input.Password placeholder={t('profile.unbind_password')} id={`unbind_pwd_${item.key}`} />}
                      onConfirm={() => {
                        const pwd = (document.getElementById(`unbind_pwd_${item.key}`) as HTMLInputElement)?.value;
                        if (!pwd) { message.warning(t('auth.password_required')); return; }
                        handleUnbind(item.key, pwd);
                      }}
                      okText={t('profile.unbind_ok')} cancelText={t('common.cancel')}>
                      <Button type="link" danger>{t('profile.unbind')}</Button>
                    </Popconfirm>
                  ) : null}
                </Space>
              }>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, width: 120 }}>
                <span style={{ color: subText }}>{item.icon}</span>
                <Text style={{ color: subText }}>{item.label}</Text>
              </div>
              <div style={{ flex: 1 }}><Text style={{ color: mainText }}>{item.value}</Text></div>
            </List.Item>
          )}
        />
      </Card>

      <Modal
        title={getModalTitle()}
        open={isModalVisible}
        onCancel={() => setIsModalVisible(false)}
        onOk={handleModalOk}
        okText={t('common.save')}
        cancelText={t('common.cancel')}
        footer={modalType === 'bind_wechat' ? null : undefined}
      >
        {!isBindModal && modalType !== 'bind_wechat' ? (
          <Form form={form} layout="vertical" onFinish={handleUpdate}>
            {renderModalContent()}
          </Form>
        ) : (
          renderModalContent()
        )}
      </Modal>

      {/* 1. 独立美化版用户实名详细信息弹窗（纯展示无禁用灰框，支持证件预览与重新实名入口） */}
      <UserKycDetailViewer
        open={kycDetailModalOpen}
        onClose={() => setKycDetailModalOpen(false)}
        onReKyc={() => {
          setKycDetailModalOpen(false);
          openKycForm(modalKycType, true);
        }}
        kyc={(modalKycType === 'enterprise' ? enterpriseKyc : personalKyc) || (kycList.find(k => k.kyc_type === modalKycType) || kycList[0] || null)}
        isEn={isEn}
      />

      {/* 2. 完整实名认证 / 重新实名表单弹窗（全字段可编辑，支持选择实名主体类型，完整提交流程） */}
      <Modal
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <IdcardOutlined style={{ color: '#2563eb' }} />
            <span style={{ fontWeight: 600 }}>
              {isReKycMode
                ? (isEn ? 'Re-verify Real-Name' : '重新实名认证')
                : (isEn ? 'Real-Name Verification' : '实名认证')}
            </span>
          </div>
        }
        open={kycFormModalOpen}
        onCancel={() => {
          setKycFormModalOpen(false);
          setIsReKycMode(false);
        }}
        confirmLoading={kycSaving}
        width={680}
        destroyOnClose
        footer={
          <Space>
            <Button
              onClick={() => {
                setKycFormModalOpen(false);
                setIsReKycMode(false);
              }}
              style={{ borderRadius: 6 }}
            >
              {t('common.cancel', '取消')}
            </Button>
            <Button
              type="primary"
              loading={kycSaving}
              onClick={handleSubmitKyc}
              style={{ borderRadius: 6 }}
            >
              {isReKycMode
                ? (isEn ? 'Submit Re-verification' : '提交重新实名')
                : (isEn ? 'Submit Verification' : '提交审核')}
            </Button>
          </Space>
        }
      >
        <Form form={kycForm} layout="vertical">
          <UserKycFormFields
            form={kycForm}
            mode="user"
            readOnly={false}
            currentStatus={modalKycType === 'enterprise' ? (enterpriseKyc?.status as UserKycStatus) : (personalKyc?.status as UserKycStatus)}
            kycEnabled={kycEnabled}
            personalKyc={personalKyc}
            enterpriseKyc={enterpriseKyc}
            onKycTypeChange={handleModalTypeChange}
            isReKyc={isReKycMode}
            hideTypeRadio={false}
          />
        </Form>
      </Modal>
    </div>
  );
};

export default Profile;
