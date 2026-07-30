import React from 'react';

export interface QuotaStatsDisplayProps {
  totalTokensIn: number;
  totalTokensOut: number;
  maxQuotaTokens?: number;
}

export const QuotaStatsDisplay: React.FC<QuotaStatsDisplayProps> = ({
  totalTokensIn,
  totalTokensOut,
  maxQuotaTokens = 1000000,
}) => {
  const totalUsed = totalTokensIn + totalTokensOut;
  const percentage = Math.min(100, Math.round((totalUsed / maxQuotaTokens) * 100));

  return (
    <div style={containerStyle}>
      <div style={headerStyle}>
        <span>⚡ Token Metering & Cost Quota</span>
        <span>{totalUsed.toLocaleString()} / {maxQuotaTokens.toLocaleString()} tokens</span>
      </div>
      <div style={barBackgroundStyle}>
        <div style={{ ...barFillStyle, width: `${percentage}%` }} />
      </div>
      <div style={subtextStyle}>
        <span>In: {totalTokensIn.toLocaleString()}</span>
        <span>Out: {totalTokensOut.toLocaleString()}</span>
      </div>
    </div>
  );
};

const containerStyle: React.CSSProperties = {
  padding: '8px 12px',
  borderRadius: '6px',
  backgroundColor: '#181825',
  border: '1px solid #313244',
  margin: '8px 0',
  fontSize: '0.8rem',
  color: '#cdd6f4',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  marginBottom: '6px',
  fontWeight: 500,
};

const barBackgroundStyle: React.CSSProperties = {
  width: '100%',
  height: '6px',
  borderRadius: '3px',
  backgroundColor: '#313244',
  overflow: 'hidden',
  marginBottom: '4px',
};

const barFillStyle: React.CSSProperties = {
  height: '100%',
  backgroundColor: '#a6e3a1',
  borderRadius: '3px',
  transition: 'width 0.3s ease',
};

const subtextStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  fontSize: '0.72rem',
  color: '#a6adc8',
};
