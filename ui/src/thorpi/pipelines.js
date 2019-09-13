// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Create a new Thorium pipeline.
 * @async
 * @function
 * @param {string} data - Pipeline details to submit when creating new pipeline
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const createPipeline = async (data, errorHandler) => {
  return client.post('/pipelines/', data).then((res) => {
    // check for errors in the request response
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to create pipeline: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to create pipeline: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to create pipeline: Unknown Error');
    }
    // error state hit, return failed
    return false;
  });
};

/**
 * Delete a pipeline by name.
 * @async
 * @function
 * @param {string} group - Group name target pipeline is owned by
 * @param {string} pipeline - Name of pipeline being deleted
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const deletePipeline = async (group, pipeline, errorHandler) => {
  const url = '/pipelines/' + group + '/' + pipeline;
  return client.delete(url).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete pipeline ${pipeline} in ${group}: Permission Denied`);
      // error is returned with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to delete pipeline ${pipeline} in ${group}: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler(`Failed to delete pipeline ${pipeline} in ${group}: Unknown Error`);
    }
    // error state hit, return failed
    return false;
  });
};

/**
 * Get details for a pipeline.
 * @async
 * @function
 * @param {string} group - group name target pipeline is owned by
 * @param {string} pipeline - name of pipeline to retrieve details about
 * @param {object} errorHandler - error handler function
 * @returns {object} - pipeline details
 */
const getPipeline = async (group, pipeline, errorHandler) => {
  const url = '/pipelines/data/' + group + '/' + pipeline;

  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get pipeline ${pipeline} in ${group}: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get pipeline ${pipeline} in ${group}: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler(`Failed to get pipeline ${pipeline} in ${group}: Unknown Error`);
    }
    return false;
  });
};

/**
 * Get a list of pipelines with optional details for a particular group.
 * @async
 * @function
 * @param {string} group - group name to list owned pipelines for
 * @param {object} errorHandler - error handler function
 * @param {boolean} details - whether to return details for listed pipelines
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - number of pipelines to return
 * @returns {object} - pipelines list with optional details
 */
const listPipelines = async (group, errorHandler, details = false, cursor = null, limit = 100) => {
  let url = '/pipelines/list/' + group + '/';
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
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to list pipelines in group ${group}: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to list pipelines in group ${group}: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler(`Failed to list pipelines in group ${group}: Unknown Error`);
    }
    return false;
  });
};

/**
 * Update details for a pipeline.
 * @async
 * @function
 * @param {string} group - Group name target pipeline is owned by
 * @param {string} pipeline - Name of pipeline to patch
 * @param {object} data - Json body to patch pipeline with
 * @param {object} errorHandler - error handler function
 * @returns {object} - request response
 */
const updatePipeline = async (group, pipeline, data, errorHandler) => {
  const url = '/pipelines/' + group + '/' + pipeline;
  return client.patch(url, data).then((res) => {
    // response was successful, reload pipeline resource
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update ${pipeline} in ${group}: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to update ${pipeline} in ${group}: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler(`Failed to update ${pipeline} in ${group}: Unknown Error`);
    }
    // error state hit, return failed
    return false;
  });
};

export { createPipeline, deletePipeline, getPipeline, listPipelines, updatePipeline };
