import React from 'react';
import { Helmet, HelmetProvider } from 'react-helmet-async';

// project imports
import { Banner } from '@components';

const NotFoundContainer = () => {
  return (
    <HelmetProvider>
      <Helmet>
        <title>Not Found &middot; Thorium</title>
      </Helmet>
      <div className="d-flex justify-content-center viewport-center">
        <div className="d-flex align-items-center" >
          <img src="/ferris-scientist.png" className="pe-4 icon-separator-end" alt="FerrisScientist" height="200px" />
        </div>
        <div className="d-flex flex-column justify-content-center ms-4">
          <Banner>Uh Oh!</Banner>
          <Banner>{window.location.pathname}</Banner>
          <Banner>Not Found</Banner>
        </div>
      </div>
    </HelmetProvider>
  );
};

export default NotFoundContainer;
