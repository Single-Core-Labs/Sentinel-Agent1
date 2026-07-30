import React, { useState } from 'react';

export interface AskUserDialogProps {
  requestId: string;
  prompt: string;
  options: string[];
  allowCustom?: boolean;
  onSubmit: (requestId: string, selection: string) => void;
  onClose: () => void;
}

export const AskUserDialog: React.FC<AskUserDialogProps> = ({
  requestId,
  prompt,
  options,
  allowCustom = true,
  onSubmit,
  onClose,
}) => {
  const [selectedOption, setSelectedOption] = useState<string>(options[0] || '');
  const [customText, setCustomText] = useState<string>('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const finalValue = customText.trim() !== '' ? customText.trim() : selectedOption;
    onSubmit(requestId, finalValue);
  };

  return (
    <div style={overlayStyle}>
      <div style={modalStyle}>
        <div style={headerStyle}>
          <h3>❓ Form Request</h3>
          <button style={closeButtonStyle} onClick={onClose}>✕</button>
        </div>
        <p style={promptStyle}>{prompt}</p>

        <form onSubmit={handleSubmit}>
          <div style={optionsContainerStyle}>
            {options.map((opt, idx) => (
              <label key={idx} style={optionLabelStyle}>
                <input
                  type="radio"
                  name="ask-user-opt"
                  value={opt}
                  checked={selectedOption === opt && customText === ''}
                  onChange={() => {
                    setSelectedOption(opt);
                    setCustomText('');
                  }}
                />
                <span style={{ marginLeft: '8px' }}>{opt}</span>
              </label>
            ))}
          </div>

          {allowCustom && (
            <div style={{ marginTop: '12px' }}>
              <label style={{ display: 'block', fontSize: '0.85rem', marginBottom: '4px', color: 'var(--text-secondary)' }}>
                Or enter custom write-in response:
              </label>
              <input
                type="text"
                value={customText}
                placeholder="Custom response..."
                onChange={(e) => setCustomText(e.target.value)}
                style={inputStyle}
              />
            </div>
          )}

          <div style={actionsStyle}>
            <button type="button" onClick={onClose} style={cancelButtonStyle}>
              Cancel
            </button>
            <button type="submit" style={submitButtonStyle}>
              Submit Choice
            </button>
          </div>
        </form>
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
  width: '440px',
  maxWidth: '90%',
  border: '1px solid #45475a',
  boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginBottom: '12px',
};

const promptStyle: React.CSSProperties = {
  fontSize: '0.95rem',
  marginBottom: '16px',
  color: '#cba6f7',
};

const optionsContainerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
};

const optionLabelStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  padding: '8px',
  borderRadius: '4px',
  backgroundColor: '#313244',
  cursor: 'pointer',
};

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '8px 12px',
  borderRadius: '4px',
  border: '1px solid #45475a',
  backgroundColor: '#181825',
  color: '#cdd6f4',
};

const actionsStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'flex-end',
  gap: '8px',
  marginTop: '16px',
};

const closeButtonStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  color: '#a6adc8',
  cursor: 'pointer',
  fontSize: '1rem',
};

const cancelButtonStyle: React.CSSProperties = {
  padding: '6px 12px',
  borderRadius: '4px',
  border: '1px solid #45475a',
  backgroundColor: 'transparent',
  color: '#a6adc8',
  cursor: 'pointer',
};

const submitButtonStyle: React.CSSProperties = {
  padding: '6px 14px',
  borderRadius: '4px',
  border: 'none',
  backgroundColor: '#89b4fa',
  color: '#11111b',
  fontWeight: 'bold',
  cursor: 'pointer',
};
