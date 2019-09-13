// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Get Thorium API and UI version
 * @async
 * @function
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const getVersion = async (errorHandler) => {
  const url = '/version';
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get Thorium version: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get Thorium version: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get Thorium version: Unknown Error');
    }
    return false;
  });
};

/**
 * Get Thorium banner
 * @async
 * @function
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const getBanner = async (errorHandler) => {
  const url = '/banner';
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get Thorium banner: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get Thorium banner: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get Thorium banner: Unknown Error');
    }
    return false;
  });
};

export { getBanner, getVersion };
