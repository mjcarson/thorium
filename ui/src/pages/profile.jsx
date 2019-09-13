import React, { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Helmet, HelmetProvider } from 'react-helmet-async';
import { Badge, Button, Col, Container, Card, Form, Modal, Row } from 'react-bootstrap';
import Avatar from '@mui/material/Avatar';

// project imports
import { Subtitle } from '@components';
import { getThoriumRole, useAuth } from '@utilities';
import { updateUser } from '@thorpi';

const Themes = ['Dark', 'Light', 'Ocean', 'Automatic']; // TODO: 'Crab', 'Custom'

const ProfileContainer = () => {
  const [tokenShowing, setTokenShowing] = useState(false);
  const [showRevokeTokenModal, setShowRevokeTokenModal] = useState(false);
  const handleCloseRevokeTokenModal = () => setShowRevokeTokenModal(false);
  const handleShowRevokeTokenModal = () => setShowRevokeTokenModal(true);
  const { revoke, userInfo } = useAuth();
  const navigate = useNavigate();

  // call thorium logout function and then
  const handleRevoke = () => {
    revoke().then(() => {
      navigate('/');
    });
  };

  const updateTheme = async (theme) => {
    const settings = { settings: { theme: theme } };
    updateUser(settings, console.log).then(() => {
      refreshUserInfo(true);
    });
  };

  return (
    <HelmetProvider>
      <Container className="d-flex justify-content-center">
        <Helmet>
          <title>Profile &middot; Thorium</title>
        </Helmet>
        <Card className="profile">
          <Card.Body>
            <Row className="d-flex justify-content-center">
              <Avatar sx={{ width: 150, height: 150 }} />
            </Row>
            <Row>
              <center>
                <h2>{userInfo.username}</h2>
              </center>
            </Row>
            <hr />
            <Row>
              <Col className="md-width" xs={2}>
                <Subtitle>Groups</Subtitle>
              </Col>
              <Col>
                {userInfo.groups &&
                  userInfo.groups.sort().map((group, idx) => (
                    <Badge key={idx} pill bg="" className="bg-blue px-3 py-2 me-1">
                      {group}
                    </Badge>
                  ))}
              </Col>
            </Row>
            <hr />
            <Row>
              <Col className="md-width" xs={2}>
                <Subtitle>Role</Subtitle>
              </Col>
              <Col>
                {userInfo.role && userInfo.role == 'Admin' && (
                  <Badge pill bg="" className="bg-maroon px-3 py-2">
                    {getThoriumRole(userInfo.role)}
                  </Badge>
                )}
                {userInfo.role && userInfo.role == 'Developer' && (
                  <Badge pill bg="" className="bg-corn-flower px-3 py-2">
                    {getThoriumRole(userInfo.role)}
                  </Badge>
                )}
                {userInfo.role && userInfo.role == 'User' && (
                  <Badge pill bg="" className="bg-cadet px-3 py-2">
                    {getThoriumRole(userInfo.role)}
                  </Badge>
                )}
              </Col>
            </Row>
            <hr />
            <Row className="flex-nowrap">
              <Col className="md-width" xs={2}>
                <Subtitle>Token</Subtitle>
              </Col>
              <Col xs={10}>
                <Row>
                  <Col>
                    <p>
                      {tokenShowing ? (
                        <p>{userInfo.token}</p>
                      ) : (
                        <p className="hidden">****************************************************************</p>
                      )}
                    </p>
                  </Col>
                </Row>
              </Col>
            </Row>
            <Row>
              <Col className="d-flex justify-content-center">
                <Button className="primary-btn" onClick={() => setTokenShowing(!tokenShowing)}>
                  {tokenShowing ? 'Hide' : 'Show'}
                </Button>
                <Button className="danger-btn" onClick={() => handleShowRevokeTokenModal()}>
                  Revoke
                </Button>
              </Col>
            </Row>
            <hr />
            <Row>
              <Col className="md-width" xs={2}>
                <Subtitle>Token Expiry</Subtitle>
              </Col>
              <Col>
                <p>{userInfo.token_expiration}</p>
              </Col>
            </Row>
            <hr />
            <Row>
              <Col className="md-width" xs={2}>
                <Subtitle>Theme</Subtitle>
              </Col>
              <Col className="d-flex justify-content-start">
                <Form>
                  <Form.Group>
                    <Form.Select
                      value={userInfo && userInfo['settings'] ? userInfo.settings['theme'] : ''}
                      onChange={(e) => updateTheme(String(e.target.value))}
                    >
                      {Themes.map((theme) => (
                        <option key={theme} value={theme}>
                          {theme}
                        </option>
                      ))}
                    </Form.Select>
                  </Form.Group>
                </Form>
              </Col>
            </Row>
            <Modal show={showRevokeTokenModal} onHide={handleCloseRevokeTokenModal}>
              <Modal.Header closeButton>
                <Modal.Title>Revoke Your Token?</Modal.Title>
              </Modal.Header>
              <Modal.Body>
                Revoking your token will automatically log you out of this page and any currently running or queued analysis jobs
                (reactions) may fail. Are you sure?
              </Modal.Body>
              <Modal.Footer className="d-flex justify-content-center">
                <Button className="danger-btn" onClick={() => handleRevoke()}>
                  Confirm
                </Button>
              </Modal.Footer>
            </Modal>
          </Card.Body>
        </Card>
      </Container>
    </HelmetProvider>
  );
};

export default ProfileContainer;
