// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Get results for a hash.
 * @async
 * @function
 * @param {string} sha256 - hash of sample to get results for
 * @param {object} errorHandler - error handler function
 * @param {object} data - search parameters to retrieve results can include
 *     groups: list of groups results can be member of
 *     tools: list of tools to get results for
 * @returns {object} - results dict
 */
const getResults = async (sha256, errorHandler, data = {}) => {
  const url = '/files/results/' + sha256;
  return client.get(url, data).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get results: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get results: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get results: Unknown Error');
    }
    return false;
  });
};

/**
 * Get results for a hash.
 * @async
 * @function
 * @param {string} sha256 - hash of sample to get results for
 * @param {string} tool - name of image that created the result
 * @param {string} id - id of the result
 * @param {string} name - name of result file to retrieve
 * @param {object} errorHandler - error handler function
 * @returns {object} - results dict
 */
const getResultsFile = async (sha256, tool, id, name, errorHandler) => {
  const url = `/files/result-files/${sha256}/${tool}/${id}`;
  // add the name of the result file to our params
  const data = {
    result_file: name
  }
  return client.get(url, { params: data, responseType: 'arraybuffer' }).then((res) => {
    if (res && res.status && res.status == 200) {
      // pass back full response because data and content type headers are needed
      return res;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get results file: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get results file: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get results file: Unknown Error');
    }
    return false;
  });
};

export { getResults, getResultsFile };
