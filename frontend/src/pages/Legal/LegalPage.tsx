/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React, { useEffect, useState } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { Spin, Typography, ConfigProvider } from 'antd';
import { useTranslation } from 'react-i18next';
import request from '../../utils/request';
import useSettingsStore from '../../store/settings';
import { sanitizeHtml } from '../../utils/sanitize';
import { DEFAULT_TOS_ZH, DEFAULT_TOS_EN, DEFAULT_PRIVACY_ZH, DEFAULT_PRIVACY_EN } from '../../constants/agreements';

const LegalPage: React.FC = () => {
  const { type } = useParams<{ type: string }>();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { i18n } = useTranslation();
  const requestedLanguage = searchParams.get('lang');
  const legalLanguage = requestedLanguage === 'en' || requestedLanguage === 'zh'
    ? requestedLanguage
    : null;
  const activeLang = (requestedLanguage || i18n.language || 'zh').toLowerCase();
  const isEn = !activeLang.startsWith('zh');

  const storeSettings = useSettingsStore((s) => s.settings);

  const resolveData = (agreement: any, copyrightText: string) => {
    const defaultTos = isEn ? DEFAULT_TOS_EN : DEFAULT_TOS_ZH;
    const defaultPrivacy = isEn ? DEFAULT_PRIVACY_EN : DEFAULT_PRIVACY_ZH;

    if (type === 'terms') {
      const pageTitle = isEn ? 'Terms of Service' : '服务条款 (Terms of Service)';
      if (isEn ? agreement?.tos_mode_en === 'link' : agreement?.tos_mode === 'link') {
        const link = isEn && agreement?.tos_link_en ? agreement.tos_link_en : agreement?.tos_link;
        if (link) {
          try {
            const parsed = new URL(link);
            if (['http:', 'https:'].includes(parsed.protocol)) {
              window.location.replace(link);
              return null;
            }
          } catch { /* 非法 URL，忽略 */ }
        }
      }
      const pageContent = isEn && agreement?.tos_content_en ? agreement.tos_content_en : (agreement?.tos_content || defaultTos);
      return { title: pageTitle, content: pageContent, copyright: String(copyrightText || '').trim() };
    } else {
      const pageTitle = isEn ? 'Privacy Policy' : '隐私政策 (Privacy Policy)';
      if (isEn ? agreement?.privacy_mode_en === 'link' : agreement?.privacy_mode === 'link') {
        const link = isEn && agreement?.privacy_link_en ? agreement.privacy_link_en : agreement?.privacy_link;
        if (link) {
          try {
            const parsed = new URL(link);
            if (['http:', 'https:'].includes(parsed.protocol)) {
              window.location.replace(link);
              return null;
            }
          } catch { /* 非法 URL，忽略 */ }
        }
      }
      const pageContent = isEn && agreement?.privacy_content_en ? agreement.privacy_content_en : (agreement?.privacy_content || defaultPrivacy);
      return { title: pageTitle, content: pageContent, copyright: String(copyrightText || '').trim() };
    }
  };

  const initialResolved = storeSettings?.agreement ? resolveData(storeSettings.agreement, storeSettings.site?.copyright || '') : null;

  const [loading, setLoading] = useState(!initialResolved);
  const [content, setContent] = useState(initialResolved?.content || '');
  const [title, setTitle] = useState(initialResolved?.title || '');
  const [copyright, setCopyright] = useState(initialResolved?.copyright || '');

  useEffect(() => {
    if (legalLanguage && i18n.language !== legalLanguage) {
      void i18n.changeLanguage(legalLanguage);
    }
  }, [legalLanguage, i18n]);

  useEffect(() => {
    if (title) {
      const siteTitle = storeSettings?.site?.title?.trim();
      document.title = siteTitle ? `${title}-${siteTitle}` : title;
    }
  }, [title, storeSettings?.site?.title]);

  useEffect(() => {
    if (type !== 'terms' && type !== 'privacy') {
      navigate('/dashboard');
      return;
    }

    // 若 store 中已有数据，则无需发网络请求，直接解析渲染
    if (storeSettings?.agreement) {
      const resolved = resolveData(storeSettings.agreement, storeSettings.site?.copyright || '');
      if (resolved) {
        setTitle(resolved.title);
        setContent(resolved.content);
        setCopyright(resolved.copyright);
        setLoading(false);
      }
      return;
    }

    const fetchLegalContent = async () => {
      const defaultTos = isEn ? DEFAULT_TOS_EN : DEFAULT_TOS_ZH;
      const defaultPrivacy = isEn ? DEFAULT_PRIVACY_EN : DEFAULT_PRIVACY_ZH;

      try {
        const response = await (request.get('/settings') as any);
        const resolved = resolveData(response?.agreement, response?.site?.copyright || '');
        if (resolved) {
          setTitle(resolved.title);
          setContent(resolved.content);
          setCopyright(resolved.copyright);
        }
      } catch (error) {
        console.error('Failed to fetch legal settings:', error);
        setTitle(type === 'terms' ? (isEn ? 'Terms of Service' : '服务条款 (Terms of Service)') : (isEn ? 'Privacy Policy' : '隐私政策 (Privacy Policy)'));
        setContent(type === 'terms' ? defaultTos : defaultPrivacy);
      } finally {
        setLoading(false);
      }
    };

    fetchLegalContent();
  }, [type, navigate, i18n.language, legalLanguage, storeSettings]);

  if (loading) {
    return (
      <div style={{ height: '100vh', display: 'flex', justifyContent: 'center', alignItems: 'center', background: '#000' }}>
        <Spin size="large" />
      </div>
    );
  }

  return (
    <ConfigProvider theme={{  }}>
      <div style={{ 
        minHeight: '100vh', 
        background: '#0a0a0a', 
        color: '#e5e5e5',
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        width: '100%',
        overflowX: 'hidden'
      }}>
        <div style={{ 
          maxWidth: 860, 
          width: '100%', 
          margin: '0 auto', 
          padding: '60px 24px',
          flex: 1,
          boxSizing: 'border-box'
        }}>
          <Typography.Title level={2} style={{ color: '#fff', textAlign: 'center', marginBottom: 40, fontWeight: 600, wordBreak: 'break-word', overflowWrap: 'break-word' }}>
            {title}
          </Typography.Title>
          
          <div 
            className="ql-editor"
            style={{ 
              background: 'transparent', 
              padding: 0,
              fontSize: '15px',
              lineHeight: 1.85,
              color: 'rgba(255, 255, 255, 0.85)',
              wordBreak: 'break-word',
              overflowWrap: 'anywhere',
              whiteSpace: 'normal',
              width: '100%',
              maxWidth: '100%',
              boxSizing: 'border-box'
            }}
            dangerouslySetInnerHTML={{ __html: sanitizeHtml(content.replace(/&nbsp;/g, ' ')) }} 
          />
        </div>
        {copyright ? (
          <div style={{
            textAlign: 'center',
            fontSize: 13,
            lineHeight: 1.6,
            color: 'rgba(255, 255, 255, 0.45)',
            padding: '24px 24px 32px',
            userSelect: 'none',
          }}>
            {copyright}
          </div>
        ) : null}
      </div>
    </ConfigProvider>
  );
};

export default LegalPage;
