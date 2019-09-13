// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Create a new Thorium image.
 * @async
 * @function
 * @param {string} data - Image details to submit when creating new image
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const createImage = async (data, errorHandler) => {
  const url = '/images/';
  return client.post(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to create image: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to create image: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to create image: Unknown Error');
    }
    return false;
  });
};

/**
 * Delete an image by name.
 * @async
 * @function
 * @param {string} group - Group name target image is owned by
 * @param {string} image - Name of image being deleted
 * @param {object} errorHandler - error handler function
 * @returns {object} - Request response
 */
const deleteImage = async (group, image, errorHandler) => {
  const url = '/images/' + group + '/' + image;
  return client.delete(url).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete image: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to delete image: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete image: Unknown Error');
    }
    return false;
  });
};

/**
 * Get details for an image.
 * @async
 * @function
 * @param {string} group - Group name target image is owned by
 * @param {string} image - Name of image to retrieve details about
 * @returns {object} - Image details
 */
const getImage = async (group, image) => {
  const url = '/images/data/' + group + '/' + image;
  return client.get(url);
};

/**
 * Get a list of images with optional details for a particular group.
 * @async
 * @function
 * @param {string} group - Group name to list owned images for
 * @param {object} errorHandler - error handler function
 * @param {boolean} details - Whether to return details for listed images
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - number of images to return
 * @returns {object} - images list with optional details
 */
const listImages = async (group, errorHandler, details = false, cursor = null, limit = 100) => {
  let url = '/images/' + group + '/';
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
      errorHandler(`Failed to list images: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to list images: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list images: Unknown Error');
    }
    return false;
  });
};

/**
 * Update details for an image.
 * @async
 * @function
 * @param {string} group - Group name target image is owned by
 * @param {string} image - Name of image to patch
 * @param {object} data - Json body to patch image with
 * @param {object} errorHandler - error handler function
 * @returns {object} - image details
 */
const updateImage = async (group, image, data, errorHandler) => {
  const url = '/images/' + group + '/' + image;
  return client.patch(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update image: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to update image: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to update image: Unknown Error');
    }
    return false;
  });
};

export { createImage, deleteImage, getImage, listImages, updateImage };
