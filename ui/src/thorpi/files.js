// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

// Debugging errors randomly inserted.
// Valid values 0-100 (percentage chance of error)
const RANDOM_DEBUG_ERRORS = 0;

/**
 * Download a file.
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample to update
 * @param {object} errorHandler - error handler function
 * @param {string} archiveFormat - the archive format used for encapsulating the downloaded file
 * @param {string} archivePassword - password to unencrypt the downloaded archive
 * @returns {object} - downloaded file
 */
const getFile = async (sha256, errorHandler, archiveFormat = 'CaRT', archivePassword = 'infected') => {
  let url = '/files/sample/' + sha256 + '/download';
  const options = { responseType: 'arraybuffer' };
  // downloading a zip is a url subpath and can take a password
  if (archiveFormat == 'Encrypted ZIP') {
    url = url + '/zip';
    if (archivePassword) {
      options['params'] = { password: archivePassword };
    }
  }
  return client.get(url, options).then((res) => {
    if (res && res.status && res.status == 200) {
      // pass back full response because data and content type headers are needed
      return res;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to download file: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to download file: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to download file: Unknown Error');
    }
    return false;
  });
};

/**
 * Get file submission details.
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample info to get
 * @param {object} errorHandler - error handler function
 * @returns {object} - submission info for a file
 */
const getFileDetails = async (sha256, errorHandler) => {
  const url = '/files/sample/' + sha256;
  return client.get(url).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to get file details: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to get file details: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to get file details: Unknown Error');
    }
    return false;
  });
};

/**
 * Get a list of files by date range.
 * @async
 * @function
 * @param {object} [data] - optional request parameters which includes:
 *   - groups:  to which the files are viewable
 *   - start: start date for search range
 *   - submission: uuid for previously searched submission
 *   - end: end date for search range
 *   - limit:  the max number of submissions to return
 * @param {object} errorHandler - error handler function
 * @param {boolean} [details] - whether to return details for listed submissions
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - number of files to return
 * @returns {object} - Promise object representing a list of file details.
 */
const listFiles = async (data, errorHandler, details = false, cursor = null) => {
  // build url parameters including optional args if specified
  let url = '/files';
  if (details) {
    url += '/details';
  }
  // pass in limit and cursor value
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
      errorHandler(`Failed to list files: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to list files: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list files: Unknown Error');
    }
    return [];
  });
};

/**
 * Update file submission details.
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample to update
 * @param {object} data - tags to add (and optionally groups to add them to)
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing tags post response.
 */
const uploadTags = async (sha256, data, errorHandler) => {
  const url = '/files/tags/' + sha256;
  return client.post(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to upload tag: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to upload tag: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to upload tag: Unknown Error');
    }
    return false;
  });
};

/**
 * Delete tags
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample to update
 * @param {object} data - tags to delete (and optionally groups to delete them from)
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing tags post response.
 */
const deleteTags = async (sha256, data, errorHandler) => {
  const url = '/files/tags/' + sha256;
  return client.delete(url, { data: data }).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete tag: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to delete tag: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete tag: Unknown Error');
    }
    return false;
  });
};

/**
 * Delete a submission
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample to update
 * @param {string} id - submission id to delete
 * @param {object} groups - groups to delete from, otherwise ALL of them
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing tags post response.
 */
const deleteSubmission = async (sha256, id, groups, errorHandler) => {
  const params = {};
  params['groups'] = groups;
  const url = '/files/sample/' + sha256 + '/' + id;
  return client.delete(url, { params: params }).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete submission: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to delete submission: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete sumbission: Unknown Error');
    }
    return false;
  });
};

/**
 * Update file submission details.
 * @async
 * @function
 * @param {string} sha256 - sha256 hash of sample to update
 * @param {object} data - data to update submission with
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing details for a file.
 */
const updateFileSubmission = async (sha256, data, errorHandler) => {
  const url = '/files/sample/' + sha256;
  return client.patch(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update submission: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to update submission: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to update submission: Unknown Error');
    }
    return false;
  });
};

/**
 * Upload a file.
 * @async
 * @function
 * @param {object} form - form object containing file and submission info
 * @param {object} errorHandler - error handler function
 * @param {object} progressHandler - progress report handler function
 * @param {object} controller - abort controller
 * @returns {object} - promise object representing file post response
 */
const uploadFile = async (form, errorHandler, progressHandler, controller) => {
  const url = '/files/';
  const config = {
    onUploadProgress: (progressEvent) => progressHandler(progressEvent.progress),
    signal: controller.signal,
  };
  if (RANDOM_DEBUG_ERRORS) {
    if (Math.floor(Math.random() * 100) < RANDOM_DEBUG_ERRORS) {
      errorHandler(`Failed to upload file: Permission Denied`);
      return false;
    }
  }
  return client.post(url, form, config).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      if ('sha256' in res.data) {
        return res.data;
      } else {
        // got a valid response but it didn't contain a sha256, probably a proxy error
        errorHandler('Error: file upload response did not contain a hash (proxy error?).');
      }
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to upload file: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.status == 409 && res.data && res.data.error) {
      return { sha256: res.data.error };
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to upload file: ${res.status}`);
      // Received abort signal
    } else if (controller.signal.aborted) {
      errorHandler('Upload cancelled by user');
      // no message or error returned
    } else {
      errorHandler('Failed to upload file: Unknown Error');
    }
    return false;
  });
};

export { deleteTags, deleteSubmission, getFile, getFileDetails, listFiles, updateFileSubmission, uploadTags, uploadFile };
