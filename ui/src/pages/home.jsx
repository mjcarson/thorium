import React from 'react';
import { Helmet, HelmetProvider } from 'react-helmet-async';
import { Col, Container, Row } from 'react-bootstrap';

// project imports
import { Banner, Search } from '@components';

const HomeContainer = () => {
  return (
    <HelmetProvider>
      <Container>
        <Helmet>
          <title>Thorium</title>
        </Helmet>
        <Row>
          <Col className="d-flex justify-content-center">
            <img src="/ferris-scientist.png" alt="FerrisScientist" width="125px" />
          </Col>
        </Row>
        <Row>
          <Col className="d-flex justify-content-center">
            <Banner>Thorium</Banner>
          </Col>
        </Row>
        <Search />
      </Container>
    </HelmetProvider>
  );
};

export default HomeContainer;
