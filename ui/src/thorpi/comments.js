// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Submit a comment about a file.
 * @async
 * @function
 * @param {string} sha256 - the hash of the sample the comment is about
 * @param {object} postForm - json formatted params to post including comment
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing comment post response
 */
const postFileComments = async (sha256, postForm, errorHandler) => {
  const url = `/files/comment/${sha256}`;
  return client.post(url, postForm).then((res) => {
    if (res && res.status && res.status == 200 && res.data) {
      return res.data;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to submit comment: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to submit comment: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to submit comment: Unknown Error');
    }
    return false;
  });
};

/**
 * Download a comment attachment.
 * @async
 * @function
 * @param {object} sha256 - the sha256 hash of the sample the comment file is about
 * @param {object} commentId - the UUID of the comment to download from
 * @param {object} attachmentID - the UUID of the comment attachment to download
 * @param {object} errorHandler - error handler function
 * @returns {object} - promise object representing comment post response
 */
const downloadAttachment = async (sha256, commentId, attachmentID, errorHandler) => {
  const url = `/files/comment/download/${sha256}/${commentId}/${attachmentID}`;
  return client.get(url, { responseType: 'arraybuffer' }).then((res) => {
    if (res && res.status && res.status == 200) {
      // pass back full response because data and content type headers are needed
      return res;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to download comment attachment: Permission Denied`);
      // error is return with message, print response
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(res.data.error);
      // error is returned with no message, print status
    } else if (res && res.status) {
      errorHandler(`Failed to download comment attachment: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to download comment attachment: Unknown Error');
    }
    return false;
  });
};

export { postFileComments, downloadAttachment };
