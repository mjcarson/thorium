// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Create a new Thorium group.
 * @async
 * @function
 * @param {object} data - Group name and description object
 * @param {object} errorHandler - error handler function
 * @returns {object} - groups details
 */
const createGroup = async (data, errorHandler) => {
  return client.post('/groups/', data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to create group: Permission Denied`);
      // group aleady exists
    } else if (res && res.status && res.status == 409) {
      errorHandler(`Group with name "${data.name}" already exists`);
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to create group: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to create group: Unknown Error');
    }
    return false;
  });
};

/**
 * Delete a group by name.
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {object} errorHandler - error handler function
 * @returns {object} - groups details
 */
const deleteGroup = async (group, errorHandler) => {
  const url = '/groups/' + group;
  return client.delete(url).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete group: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to delete group: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete group: Unknown Error');
    }
    return false;
  });
};

/**
 * Get details for a group.
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {object} errorHandler - error handler function
 * @returns {object} - groups details
 */
const getGroup = async (group, errorHandler) => {
  const url = '/groups/' + group + '/details/';
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status == 401) {
      errorHandler(`Failed to get group: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to get group: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get group: Unknown Error');
    }
    return false;
  });
};

/**
 * Get a list of groups with optional details.
 * @async
 * @function
 * @param {object} errorHandler - error handler function
 * @param {boolean} details - Whether to return details for listed groups
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - number of groups to return
 * @returns {object} - groups list
 */
const listGroups = async (errorHandler, details = false, cursor = null, limit = 1000) => {
  let url = '/groups/';
  if (details) {
    url += 'details/';
  }
  // pass in limit and cursor value
  const params = {};
  if (cursor) {
    params['cursor'] = cursor;
  }
  params['limit'] = limit;
  return client.get(url, { params: params }).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      if (details && res.data.details) {
        return res.data.details;
      }
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status == 401) {
      errorHandler(`Failed to list groups: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to list groups: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list groups: Unknown Error');
    }
    return false;
  });
};

/**
 * Update details for a group.
 * @async
 * @function
 * @param {string} group - Name of group to update
 * @param {object} data - Json body to patch group with
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const updateGroup = async (group, data, errorHandler) => {
  const url = '/groups/' + group;
  return client.patch(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update group: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to update group: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to update group: Unknown Error');
    }
    return false;
  });
};

export { createGroup, deleteGroup, getGroup, listGroups, updateGroup };
