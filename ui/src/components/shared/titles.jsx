import React from 'react';

const Subtitle = ({ children, className }) => {
  return <div className={`subtitle ${className ? className : ''}`}>{children}</div>;
};

const SimpleSubtitle = ({ children, className }) => {
  return <div className={`simple-subtitle ${className ? className : ''}`}>{children}</div>;
};

const Title = ({ children, className, small }) => {
  if (small) {
    return <div className={`small-title ${className ? className : ''}`}>{children}</div>;
  } else {
    return <div className={`title ${className ? className : ''}`}>{children}</div>;
  }
};

const SimpleTitle = ({ children, className }) => {
  return <div className={`simple-title ${className ? className : ''}`}>{children}</div>;
};

const Banner = ({ children, className }) => {
  return <div className={`title-banner ${className ? className : ''}`}>{children}</div>;
};

export { Banner, SimpleTitle, SimpleSubtitle, Subtitle, Title };
