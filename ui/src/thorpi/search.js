// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Search results for query string
 * @async
 * @function
 * @param {string} query - The search string to query results
 * @param {object} errorHandler - error handler function
 * @param {Array} groups - A list of groups to filter results
 * @param {string} start - The start date range of filtered results
 * @param {string} end - The end date range of filtered results
 * @param {string} cursor - The UUID cursor for an existing search
 * @param {string} limit - The max number of results to return
 * @returns {object} - results object
 */
const searchResults = async (query, errorHandler, groups = null, start = null, end = null, cursor = null, limit = 100) => {
  const url = '/search/';
  // pass in params to search request
  const params = { query: query };
  if (groups) {
    params['groups'] = groups;
  }
  if (start) {
    params['start'] = start;
  }
  if (end) {
    params['end'] = end;
  }
  if (cursor) {
    params['cursor'] = cursor;
  }
  params['limit'] = limit;

  return client.get(url, { params: params }).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to search results: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to search results: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to search results: Unknown Error');
    }
    return false;
  });
};

export { searchResults };
