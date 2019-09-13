import React from 'react';
import { Alert, Button, Row, Col } from 'react-bootstrap';

// project imports
import { hasAvailableUIUpdate, reloadUI } from '@utilities';

const StateAlerts = () => {
  // alerts are currently disabled
  return null;
};

const RenderErrorAlert = ({ message }) => {
  let errorMessage =
    'Uh oh! An error occurred while rendering. If this persists after refreshing the page, please report it to your Thorium Admins.';
  if (message) {
    errorMessage = message;
  }
  return (
    <Alert variant="danger">
      <center>
        <pre>{errorMessage}</pre>
      </center>
    </Alert>
  );
};

export { StateAlerts, RenderErrorAlert };
