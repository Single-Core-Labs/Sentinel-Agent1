import React, { useState } from 'react';

export interface SessionSummaryItem {
  id: string;
  title: string;
  createdAt: number;
  lastActiveAt: number;
  totalTokens: number;
  messageCount: number;
}

export interface SessionBrowserProps {
  sessions: SessionSummaryItem[];
  currentSessionId?: string;
  onSelectSession: (sessionId: string) => void;
  onClose: () => void;
}

export const SessionBrowser: React.FC<SessionBrowserProps> = ({
  sessions,
  currentSessionId,
  onSelectSession,
  onClose,
}) => {
  const [filter, setFilter] = useState('');

  const filteredSessions = sessions.filter(
    (s) =>
      s.title.toLowerCase().includes(filter.toLowerCase()) ||
      s.id.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div style={overlayStyle}>
      <div style={modalStyle}>
        <div style={headerStyle}>
          <h3>📂 Session Browser & History</h3>
          <button style={closeButtonStyle} onClick={onClose}>✕</button>
        </div>

        <input
          type="text"
          placeholder="Filter sessions by title or ID..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={filterInputStyle}
        />

        <div style={listStyle}>
          {filteredSessions.length === 0 ? (
            <div style={emptyStyle}>No matching sessions found.</div>
          ) : (
            filteredSessions.map((s) => {
              const isCurrent = s.id === currentSessionId;
              return (
                <div
                  key={s.id}
                  style={{
                    ...itemStyle,
                    borderColor: isCurrent ? '#89b4fa' : '#313244',
                    backgroundColor: isCurrent ? '#313244' : '#181825',
                  }}
                  onClick={() => onSelectSession(s.id)}
                >
                  <div style={titleStyle}>{s.title || `Session ${s.id.substring(0, 8)}`}</div>
                  <div style={metaStyle}>
                    ID: {s.id.substring(0, 8)}... | Tokens: {s.totalTokens} | Messages: {s.messageCount}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
};

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  backgroundColor: 'rgba(0,0,0,0.6)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
};

const modalStyle: React.CSSProperties = {
  backgroundColor: '#1e1e2e',
  color: '#cdd6f4',
  padding: '20px',
  borderRadius: '8px',
  width: '540px',
  maxWidth: '92%',
  maxHeight: '80vh',
  display: 'flex',
  flexDirection: 'column',
  border: '1px solid #45475a',
  boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginBottom: '12px',
};

const closeButtonStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  color: '#a6adc8',
  cursor: 'pointer',
  fontSize: '1rem',
};

const filterInputStyle: React.CSSProperties = {
  padding: '8px 12px',
  borderRadius: '4px',
  border: '1px solid #45475a',
  backgroundColor: '#181825',
  color: '#cdd6f4',
  marginBottom: '12px',
};

const listStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
  overflowY: 'auto',
  paddingRight: '4px',
};

const itemStyle: React.CSSProperties = {
  padding: '10px 12px',
  borderRadius: '6px',
  border: '1px solid',
  cursor: 'pointer',
  transition: 'background-color 0.15s ease',
};

const titleStyle: React.CSSProperties = {
  fontWeight: 'bold',
  fontSize: '0.95rem',
  color: '#89b4fa',
  marginBottom: '4px',
};

const metaStyle: React.CSSProperties = {
  fontSize: '0.78rem',
  color: '#a6adc8',
};

const emptyStyle: React.CSSProperties = {
  textAlign: 'center',
  padding: '24px',
  color: '#a6adc8',
  fontSize: '0.9rem',
};
