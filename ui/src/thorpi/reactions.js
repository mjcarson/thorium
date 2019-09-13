// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

// Debugging errors randomly inserted.
// Valid values 0-100 (percentage chance of error)
const RANDOM_DEBUG_ERRORS = 0;

/**
 * Submit reactions for a file.
 * @async
 * @function
 * @param {object} reaction - Reactions to submit
 * @param {object} errorHandler - error handler function
 * @param {Map} tags - An array of tags to add to each reaction in list
 * @returns {object} - promise object representing reaction submission response
 */
const createReaction = async (reaction, errorHandler, tags = null) => {
  const url = '/reactions/';
  if (tags != null) {
    reaction['tags'] = tags;
  }
  if (RANDOM_DEBUG_ERRORS) {
    if (Math.floor(Math.random() * 100) < RANDOM_DEBUG_ERRORS) {
      errorHandler(`Failed to create reaction: Permission Denied`);
      return false;
    }
  }
  return client.post(url, reaction).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to create reaction: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to create reaction: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to create reaction: Unknown Error');
    }
  });
};

/**
 * Get details for a reaction by UUID.
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {string} uuid - ID of reaction to get
 * @param {object} errorHandler - error handler function
 * @returns {object} - groups details
 */
const getReaction = async (group, uuid, errorHandler) => {
  const url = '/reactions/' + group + '/' + uuid;
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get reaction: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get reaction: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get reaction: Unknown Error');
    }
    return false;
  });
};

/**
 * Get logs for a reaction
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {string} uuid - ID of reaction to get
 * @param {object} errorHandler - error handler function
 * @param {number} cursor - The number of log lines to skip
 * @param {number} limit - The number of log lines to retrieve
 * @returns {object} - reaction logs
 */
const getReactionLogs = async (group, uuid, errorHandler, cursor = null, limit = 100) => {
  const url = '/reactions/logs/' + group + '/' + uuid;
  // pass in limit and cursor value
  const params = {};
  if (cursor) {
    params['cursor'] = cursor;
  }
  params['limit'] = limit;
  return client.get(url, { params: params }).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get reaction logs: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get reaction logs: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get reaction logs: Unknown Error');
    }
    return false;
  });
};

/**
 * Get logs for a reaction stage
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {string} uuid - ID of reaction to get
 * @param {string} stage - Name of stage (image) within reaction
 * @param {object} errorHandler - error handler function
 * @param {number} cursor - The number of log lines to skip
 * @param {number} limit - The number of log lines to retrieve
 * @returns {object} - reaction logs
 */
const getReactionStageLogs = async (group, uuid, stage, errorHandler, cursor = null, limit = 100) => {
  const url = '/reactions/logs/' + group + '/' + uuid + '/' + stage;
  // pass in limit and cursor value
  const params = {};
  if (cursor) {
    params['cursor'] = cursor;
  }
  params['limit'] = limit;
  return client.get(url, { params: params }).then((res) => {
    if (res && res.status && res.status == 200 && res.data && res.data.logs) {
      return res.data.logs;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get reaction stage logs: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get reaction stage logs: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get reaction stage logs: Unknown Error');
    }
    return false;
  });
};

/**
 * Get a list of reactions with optional details.
 * @async
 * @function
 * @param {string} group - group that pipeline is a member of
 * @param {object} errorHandler - error handler function
 * @param {string} [pipeline] - name of pipeline to get reactions for
 * @param {string} [tag] - tag to get reactions for
 * @param {boolean} [details] - whether to return details for listed pipelines
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - number of reactions to return
 * @returns {object} - reactions list
 */
const listReactions = async (group, errorHandler, pipeline = '', tag = '', details = false, cursor = null, limit = 1000) => {
  let url = '/reactions/';
  if (tag == '') {
    url += 'list/' + group + '/' + pipeline + '/';
  } else {
    url += 'tag/' + group + '/' + tag + '/';
  }
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
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to list reactions: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to list reactions: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list reactions: Unknown Error');
    }
    return false;
  });
};

/**
 * Delete reaction by UUID.
 * @async
 * @function
 * @param {string} group - Group to get details about
 * @param {string} uuid - ID of reaction to get
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing reaction post response.
 */
const deleteReaction = async (group, uuid, errorHandler) => {
  const url = '/reactions/' + group + '/' + uuid;
  return client.delete(url).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete reaction: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to delete reaction: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete reaction: Unknown Error');
    }
    return false;
  });
};

export { createReaction, getReaction, listReactions, getReactionLogs, getReactionStageLogs, deleteReaction };
