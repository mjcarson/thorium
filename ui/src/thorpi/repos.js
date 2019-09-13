// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Get a list of repos by date range.
 * @async
 * @function
 * @param {object} [data] - optional request parameters which includes:
 *   - groups: to which the repos are viewable
 *   - start: start date for search range
 *   - end: end date for search range
 *   - limit:  the max number of submissions to return
 * @param {object} errorHandler - error handler function
 * @param {boolean} [details] - whether to return details for listed submissions
 * @param {string} cursor - the cursor value to continue listing from
 * @returns {object} - Promise object representing a list of file details.
 */
const listRepos = async (data, errorHandler, details = false, cursor = null) => {
  // build url parameters including optional args if specified
  let url = '/repos';
  if (details) {
    url += '/details/';
  }
  // pass in cursor value
  if (cursor) {
    data['cursor'] = cursor;
  }
  return client.get(url, { params: data }).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      if (details && res.data.details) {
        return res.data.details;
      }
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to list repos: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to list repos: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list repos: Unknown Error');
    }
    return [];
  });
};

export { listRepos };
