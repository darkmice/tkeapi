/*
 * tkeapi (tokensbyte) opensource
 * © 2026 tkeapi.com
 * @copyright      Copyright netbcloud/wstianxia 
 * @license        MIT (https://www.tkeapi.com/)
 */

import React from 'react';
import { Typography, Button, Tooltip } from 'antd';
import { SettingOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import { type ClassificationCount } from '../../types';
import { useThemeStore } from '../../store/theme';
import { Image as ImageIcon, Video, AudioLines, MessageSquare, Cuboid, LayoutGrid, ListOrdered, Sparkles } from 'lucide-react';
import SmartSvgIcon from '../SmartSvgIcon';

const { Text } = Typography;

interface ClassificationFilterProps {
  providers: ClassificationCount[];
  apiProviders?: ClassificationCount[];
  types: ClassificationCount[];
  selectedProvider: number | null;
  selectedApiProvider?: number | null;
  selectedType: number | null;
  onProviderChange: (id: number | null) => void;
  onApiProviderChange?: (id: number | null) => void;
  onTypeChange: (id: number | null) => void;
  onManageProviders?: () => void;
  onManageApiProviders?: () => void;
  onManageTypes?: () => void;
}

const ClassificationFilter: React.FC<ClassificationFilterProps> = ({
  providers,
  apiProviders,
  types,
  selectedProvider,
  selectedApiProvider,
  selectedType,
  onProviderChange,
  onApiProviderChange,
  onTypeChange,
  onManageProviders,
  onManageApiProviders,
  onManageTypes,
}) => {
  const { t, i18n } = useTranslation();
  const { themeMode } = useThemeStore();
  const isLight = themeMode === 'light';
  const isEn = i18n.language === 'en';

  const renderSystemIcon = (name: string, isLight: boolean, isSelected: boolean) => {
    const lowerName = name.toLowerCase();
    const style = { color: isSelected ? '#fff' : (isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)') };
    if (lowerName.includes('封装') || lowerName.includes('encapsulation')) return <Sparkles size={14} style={style} />;
    if (lowerName.includes('图片') || lowerName.includes('image')) return <ImageIcon size={14} style={style} />;
    if (lowerName.includes('视频') || lowerName.includes('video')) return <Video size={14} style={style} />;
    if (lowerName.includes('音频') || lowerName.includes('audio')) return <AudioLines size={14} style={style} />;
    if (lowerName.includes('聊天') || lowerName.includes('chat') || lowerName.includes('text')) return <MessageSquare size={14} style={style} />;
    if (lowerName.includes('embedding') || lowerName.includes('向量')) return <Cuboid size={14} style={style} />;
    if (lowerName.includes('rerank') || lowerName.includes('排序')) return <ListOrdered size={14} style={style} />;
    return null;
  };

  const renderFilterRow = (
    label: string,
    items: ClassificationCount[],
    selectedValue: number | null,
    onSelect: (id: number | null) => void,
    onManage?: () => void,
    isLast: boolean = false
  ) => (
    <div style={{ display: 'flex', alignItems: 'flex-start', marginBottom: isLast ? 0 : 10, padding: 0 }}>
      <Text type="secondary" style={{ width: 76, flexShrink: 0, fontSize: '13px', fontWeight: 500, paddingTop: 3, lineHeight: '20px' }}>{label}</Text>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px 8px', flexGrow: 1, alignItems: 'center' }}>
        <div
          onClick={() => onSelect(null)}
          style={{ 
            padding: '3px 10px', 
            borderRadius: 14,
            fontSize: '13.5px',
            lineHeight: '20px',
            backgroundColor: selectedValue === null ? '#1677ff' : (isLight ? '#f2f2f4' : '#1f1f1f'),
            color: selectedValue === null ? '#fff' : (isLight ? 'rgba(0,0,0,0.85)' : 'rgba(255, 255, 255, 0.85)'),
            border: selectedValue === null ? '1px solid #1677ff' : (isLight ? '1px solid #e0e0e0' : '1px solid #303030'),
            cursor: 'pointer',
            display: 'inline-flex',
            alignItems: 'center',
            gap: 5,
            transition: 'all 0.15s ease',
            userSelect: 'none',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 15, height: 15 }}>
            <LayoutGrid size={14} style={{ color: selectedValue === null ? '#fff' : (isLight ? 'rgba(0,0,0,0.65)' : 'rgba(255,255,255,0.65)') }} />
          </div>
          <div style={{ display: 'flex', alignItems: 'center' }}>
            {t('common.all')} <span style={{ opacity: 0.65, marginLeft: 4, fontSize: '12px' }}>{items.reduce((acc, item) => acc + item.count, 0)}</span>
          </div>
        </div>
        {items.map(item => (
          <div
            key={item.id}
            onClick={() => onSelect(item.id)}
            style={{ 
              padding: '3px 10px', 
              borderRadius: 14,
              fontSize: '13.5px',
              lineHeight: '20px',
              backgroundColor: selectedValue === item.id ? '#1677ff' : (isLight ? '#f2f2f4' : '#1f1f1f'),
              color: selectedValue === item.id ? '#fff' : (isLight ? 'rgba(0,0,0,0.85)' : 'rgba(255, 255, 255, 0.85)'),
              border: selectedValue === item.id ? '1px solid #1677ff' : (isLight ? '1px solid #e0e0e0' : '1px solid #303030'),
              cursor: 'pointer',
              display: 'inline-flex',
              alignItems: 'center',
              gap: 5,
              transition: 'all 0.15s ease',
              userSelect: 'none',
            }}
          >
            {(() => {
              const sysIcon = renderSystemIcon(item.name_en || item.name, isLight, selectedValue === item.id);
              if (sysIcon) {
                return <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', width: 15, height: 15 }}>{sysIcon}</div>;
              }
              if (item.logo) {
                return <SmartSvgIcon src={`/assets/icons/lobe/${item.logo}.svg`} alt="" style={{ width: 15, height: 15, objectFit: 'contain', display: 'block' }} onError={e => { (e.target as HTMLImageElement).style.display = 'none'; }} />;
              }
              return <div style={{ width: 15, height: 15, borderRadius: 3, background: 'rgba(128,128,128,0.15)', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 9.5 }}>{(item.name_en || item.name).charAt(0)}</div>;
            })()}
            <div style={{ display: 'flex', alignItems: 'center' }}>
              {(isEn && item.name_en) ? item.name_en : item.name} <span style={{ opacity: 0.65, marginLeft: 4, fontSize: '12px' }}>{item.count}</span>
            </div>
          </div>
        ))}
        {onManage && (
          <Tooltip title={t('common.manage')}>
            <Button 
              type="text" 
              size="small" 
              icon={<SettingOutlined style={{ color: '#1677ff', fontSize: 13.5 }} />} 
              onClick={onManage}
              style={{ padding: 0, height: 24, width: 24, minWidth: 24, display: 'inline-flex', alignItems: 'center', justifyContent: 'center' }}
            />
          </Tooltip>
        )}
      </div>
    </div>
  );

  const hasApiProviders = Boolean(apiProviders && onApiProviderChange);

  return (
    <div style={{ 
      backgroundColor: isLight ? '#fafafa' : '#141414', 
      padding: '10px 12px', 
      borderRadius: 8, 
      marginBottom: 12,
      border: isLight ? '1px solid #e8e8e8' : '1px solid #282828'
    }}>
      {renderFilterRow(t('models.provider', '官方服务商'), providers, selectedProvider, onProviderChange, onManageProviders, false)}
      {hasApiProviders && renderFilterRow(t('models.api_provider', 'API服务商'), apiProviders!, selectedApiProvider ?? null, onApiProviderChange!, onManageApiProviders, false)}
      {renderFilterRow(t('models.type', '类型'), types, selectedType, onTypeChange, onManageTypes, true)}
    </div>
  );
};

export default ClassificationFilter;
