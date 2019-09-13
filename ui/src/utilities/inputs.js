import { parseISO } from 'date-fns';

// safely convert formatted string to date
const safeStringToDateConversion = (date) => {
  try {
    return parseISO(date);
  } catch (e) {
    if (e instanceof RangeError) {
      // this hits if the passed in value is not a valid date
      return null;
    } else {
      throw e;
    }
  }
};

// safely convert date object to string
const safeDateToStringConversion = (date) => {
  try {
    return date.toISOString();
  } catch (e) {
    if (e instanceof RangeError) {
      // this hits if the passed in value is not a valid date
      return null;
    } else {
      throw e;
    }
  }
};

// safely parse JSON from things like session or input fields
const safeParseJSON = (unsafeJSON) => {
  try {
    return JSON.parse(unsafeJSON);
  } catch (e) {
    if (e instanceof SyntaxError) {
      // this hits if the passed in value is not a valid date
      return null;
    } else {
      throw e;
    }
  }
};

export { safeParseJSON, safeDateToStringConversion, safeStringToDateConversion };
