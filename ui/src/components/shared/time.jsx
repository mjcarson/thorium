import React, { Fragment } from 'react';

const Time = ({ children, verbose }) => {
  let date;
  let fullTime;
  let time;
  try {
    [date, fullTime] = children.split('T');
    time = fullTime.split('.')[0];
  } catch (e) {
    return <Fragment>{children}</Fragment>;
  }
  if (verbose) {
    return (
      <Fragment>
        <i>{'on '}</i>
        {date}
        <i>{' at '}</i>
        {time}
      </Fragment>
    );
  }

  return <Fragment>{date + ' ' + time}</Fragment>;
};

export default Time;
