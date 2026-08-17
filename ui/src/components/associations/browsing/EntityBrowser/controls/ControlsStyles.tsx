import styled from 'styled-components';

export const HiddenControl = styled.div`
  position: relative;
  display: inline-flex;
`;

export const HiddenMenu = styled.div`
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 20;
  min-width: 220px;
  max-width: 320px;
  max-height: 280px;
  overflow-y: auto;
  padding: 4px;
  background: var(--thorium-panel-bg);
  border: 1px solid var(--thorium-panel-border);
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
`;

export const HiddenMenuItem = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 6px;
  font-size: 0.8rem;
  color: var(--thorium-text);
  border-radius: 6px;

  &:hover {
    background: var(--thorium-highlight-panel-bg);
  }
`;

export const HiddenMenuLabel = styled.div`
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
`;

export const HiddenMenuAction = styled.button`
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  border: none;
  color: var(--thorium-highlight-text);
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 6px;

  &:hover {
    background: var(--thorium-highlight-panel-bg);
    color: var(--thorium-text);
  }
  &:focus-visible {
    outline: 2px solid var(--thorium-highlight-text);
    outline-offset: -2px;
  }
`;

export const HiddenMenuHeader = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 6px;
  border-bottom: 1px solid var(--thorium-panel-border);
  margin-bottom: 4px;
  color: var(--thorium-secondary-text);
  font-size: 0.78rem;
  font-weight: 700;
`;

export const SortControls = styled.div`
  display: inline-flex;
  align-items: center;
  gap: 6px;
`;

export const SortLabel = styled.label`
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--thorium-secondary-text);
`;

export const SortSelect = styled.select`
  font-size: 0.8rem;
  font-weight: 600;
  color: var(--thorium-text);
  background: var(--thorium-panel-bg);
  border: none;
  /* border: 1px solid var(--thorium-panel-border); */
  /* border-radius: 12px; */
  cursor: pointer;

  &:hover {
    border-color: var(--thorium-highlight-panel-border);
  }
`;

export const SelectChip = styled.div`
  padding: 4px 8px;
  background-color: var(--thorium-panel-bg);
  border: 1px solid var(--thorium-panel-border);
  border-radius: 8px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 5px;
  line-height: 1;
`;
