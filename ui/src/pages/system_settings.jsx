import React, { useEffect, useState } from 'react';
import { Helmet, HelmetProvider } from 'react-helmet-async';
import { Alert, Col, Container, Row, Table } from 'react-bootstrap';

// project imports
import { Subtitle, Title } from '@components';
import { getSystemSettings } from '@thorpi';

const SystemSettings = () => {
  const [getSettingsError, setGetSettingsError] = useState('');
  const [systemSettings, setSystemSettings] = useState({});

  // fetch latest stats from API
  const fetchSettings = async () => {
    const settings = await getSystemSettings(setGetSettingsError);
    if (settings) {
      setSystemSettings(settings);
    }
  };

  // trigger fetch stats on initial page load
  useEffect(() => {
    fetchSettings();
  }, []);

  return (
    <HelmetProvider>
      <Container className="settings">
        <Helmet>
          <title>Settings &middot; Thorium</title>
        </Helmet>
        <Row>{getSettingsError != '' && <Alert>{getSettingsError}</Alert>}</Row>
        <Row>
          <Col className="d-flex justify-content-center">
            <Title>System Settings</Title>
          </Col>
        </Row>
        <Row>
          <Col className="d-flex justify-content-center">
            <Table striped bordered hover>
              <tbody>
                {Object.keys(systemSettings).map((setting) => (
                  <tr key={setting}>
                    <td>
                      <center>
                        <Subtitle>{setting}</Subtitle>
                      </center>
                    </td>
                    <td>
                      <center>
                        <Subtitle>{systemSettings[setting].toString()}</Subtitle>
                      </center>
                    </td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </Col>
        </Row>
      </Container>
    </HelmetProvider>
  );
};

export default SystemSettings;
