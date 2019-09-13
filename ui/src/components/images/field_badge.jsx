import React from 'react';
import { Badge } from 'react-bootstrap';

// returns a badge for a given image field based on the provided color
const FieldBadge = ({ field, color }) => {
  let value = '';
  // objects must be handled differently than other string values
  // string conversion of objects are [object Object] for empty and
  // nonempty strings
  if (typeof field == 'object' && field != null) {
    if ((!Array.isArray(field) && Object.keys(field).length > 0) || (Array.isArray(field) && field.length > 0 && String(field) != '')) {
      // if object (array or dict) we do no conversions
      value = field;
    } else {
      // empty array or dict will become none value in switch
      value = '';
    }
  } else {
    value = String(field);
  }

  // Use true/false coloring if the value is a boolean
  switch (value) {
    case 'false':
      return (
        <Badge bg="" className="bg-amber">
          {value}
        </Badge>
      );
    case 'true':
      return (
        <Badge bg="" className="bg-green">
          {value}
        </Badge>
      );
    // None values from api
    case 'None':
    // empty strings or empty arrays which become empty strings with String()
    case '':
    case 'null':
    case 'undefined':
      return (
        <Badge bg="" className="bg-grey">
          {String('none')}
        </Badge>
      );
    default:
      if (Array.isArray(field) && field.length) {
        // returns one badge per item in array
        const badgeArray = field.map((item, idx) => (
          <Badge key={idx} bg="" className="me-1 image-tag" style={{ backgroundColor: '#7e7c7c' }}>
            {typeof item == 'object' && field != null ? JSON.stringify(item) : String(item)}
          </Badge>
        ));
        return badgeArray;
      } else if (typeof field == 'object') {
        const badgeArray = [];
        // if the object contains key/value pairs
        // eslint-disable-next-line guard-for-in
        Object.keys(field).forEach((itemKey, index) => {
          badgeArray.push(
            <Badge key={index} className="me-1 image-tag" style={{ backgroundColor: color }}>
              {`${itemKey}: ${field[itemKey]}`}
            </Badge>,
          );
        });
        return badgeArray;
      } else {
        return (
          <Badge bg="" className="me-1 image-tag" style={{ backgroundColor: color }}>
            {String(field)}
          </Badge>
        );
      }
  }
};

export default FieldBadge;
