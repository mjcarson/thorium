import '@styles/main.scss';

const createReactSelectStyles = (color, backgroundColor) => {
  const newStyleTemplate = {
    control: (base, state) => ({
      ...base,
      background: 'var(--thorium-secondary-panel-bg)',
      borderColor: state.isFocused ? 'var(--thorium-highlight-panel-border)' : 'var(--thorium-panel-border)',
      boxShadow: state.isFocused ? null : null,
      '&:hover': {
        borderColor: state.isFocused ? 'var(--thorium-highlight-panel-border)' : 'var(--thorium-panel-border)',
      },
    }),
    menu: (base) => ({
      ...base,
      backgroundColor: 'var(--thorium-secondary-panel-bg)',
    }),
    menuList: (base) => ({
      ...base,
      backgroundColor: 'var(--thorium-secondary-panel-bg)',
    }),
    option: (base) => ({
      ...base,
      background: 'var(--thorium-secondary-panel-bg)',
      '&:hover': {
        background: backgroundColor,
      },
    }),
    multiValue: (provided) => ({
      ...provided,
      color: color,
      backgroundColor: backgroundColor,
    }),
    multiValueLabel: (provided) => ({
      ...provided,
      color: color,
      backgroundColor: backgroundColor,
    }),
  };
  return newStyleTemplate;
};

export { createReactSelectStyles };
