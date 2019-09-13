// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Get Thorium system stats
 * @async
 * @function
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const getSystemStats = async (errorHandler) => {
  const url = '/system/stats';
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get system stats: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get system stats: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get system stats: Unknown Error');
    }
    return false;
  });
};

/**
 * Get Thorium system settings
 * @async
 * @function
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const getSystemSettings = async (errorHandler) => {
  const url = '/system/settings';
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get system settings: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get system settings: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get system settings: Unknown Error');
    }
    return false;
  });
};

export { getSystemStats, getSystemSettings };
