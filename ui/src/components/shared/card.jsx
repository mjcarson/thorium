import React from 'react';
import { Card } from 'react-bootstrap';

const ThoriumCard = ({ children, panel, className }) => {
  if (panel) {
    return <Card className={`panel ${className}`}>{children}</Card>;
  }
  return <Card className={`body ${className}`}>{children}</Card>;
};

export { ThoriumCard as Card };
