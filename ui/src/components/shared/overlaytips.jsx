import React, { useState } from 'react';
import { OverlayTrigger, Tooltip } from 'react-bootstrap';

const OverlayTip = ({ children, tip, wide, placement }) => {
  // use trigger="click" for OverlayTrigger for troubleshooting css/style issues
  const [showOverlay, setShowOverlay] = useState(false);
  const [hoveringOverlay, setHoveringOverlay] = useState(false);

  // update whether overlay tip is hovered over by mouse
  const updateHoveringOverlay = (hovering) => {
    if (hovering) {
      setHoveringOverlay(true);
      setShowOverlay(true);
    } else {
      setHoveringOverlay(false);
      setShowOverlay(false);
    }
  };

  // update whether overlay should be showing
  const updateShowOverlay = (show) => {
    if (show) {
      setShowOverlay(true);
      // don't allow closing of tip by Overlay trigger when hovering the overlay tip itself
    } else if (!hoveringOverlay && !show) {
      setShowOverlay(false);
    }
  };
  return (
    <OverlayTrigger
      show={showOverlay}
      onToggle={(show) => updateShowOverlay(show)}
      placement={placement}
      overlay={
        <Tooltip
          className={wide ? 'tooltip-wide' : ''}
          onMouseLeave={() => updateHoveringOverlay(false)}
          onMouseEnter={() => updateHoveringOverlay(true)}
        >
          {tip}
        </Tooltip>
      }
    >
      <span>{children}</span>
    </OverlayTrigger>
  );
};

const OverlayTipLeft = ({ children, tip, wide }) => {
  return (
    <OverlayTip tip={tip} wide={wide} placement="left">
      {children}
    </OverlayTip>
  );
};

const OverlayTipRight = ({ children, tip, wide }) => {
  return (
    <OverlayTip tip={tip} wide={wide} placement="right">
      {children}
    </OverlayTip>
  );
};

const OverlayTipBottom = ({ children, tip, wide }) => {
  return (
    <OverlayTip tip={tip} wide={wide} placement="bottom">
      {children}
    </OverlayTip>
  );
};

const OverlayTipTop = ({ children, tip, wide }) => {
  return (
    <OverlayTip tip={tip} wide={wide} placement="top">
      {children}
    </OverlayTip>
  );
};

export { OverlayTipTop, OverlayTipBottom, OverlayTipRight, OverlayTipLeft };
