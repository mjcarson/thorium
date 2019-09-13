import React from 'react';
import { Col, Container, Row, Spinner } from 'react-bootstrap';

const LoadingSpinner = ({ loading }) => {
  return (
    <Container hidden={!loading}>
      <Row>
        <Col className="d-flex justify-content-center m-4">
          <Spinner animation="border" className="loading" />
        </Col>
      </Row>
    </Container>
  );
};

export default LoadingSpinner;
