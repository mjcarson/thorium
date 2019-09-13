// import the base client function that loads from the config
// and injects the token via axios intercepts
import client from './client';

/**
 * Auth user by password to get a token.
 * @async
 * @function
 * @param {string} username - name of the user to auth
 * @param {string} password - the users password
 * @param {object} handleAuthErr - error handler function
 * @returns {object} - the users token and token expiration date
 */
const authUserPass = async (username, password, handleAuthErr) => {
  const url = '/users/auth';
  // build basic auth header and assign Authorization filed to value
  const header = { Authorization: 'basic ' + btoa(username + ':' + password) };

  return client.post(url, {}, { headers: header }).then((res) => {
    if (res?.status && res.status == 200 && res.data?.token) {
      // set login status and then route to previous location or home
      return res.data.token;
    } else if (res && res.data && res.data.error) {
      handleAuthErr(res.data.error);
    } else if (res && res.status && res.status == 401) {
      handleAuthErr(`Login Failed: Permission Denied`);
    } else if (res && res.status) {
      handleAuthErr(`Login failed: ${res.status}`);
    } else {
      handleAuthErr('Unparsable login error: please contact a Thorium admin');
    }
    return false;
  });
};

/**
 * Auth user by token.
 * @async
 * @function
 * @param {string} token - the users Thorium token
 * @returns {object} - the users token and token expiration date
 */
const authUserToken = async (token) => {
  // build basic auth header and assign Authorization filed to value
  const header = { Authorization: 'token ' + btoa(token) };
  return client.post('/users/auth', {}, { headers: header });
};

/**
 * Create a new user.
 * @async
 * @function
 * @param {string} name - name of user to create
 * @param {string} email - email for user
 * @param {string} password - password for new user
 * @param {string} role - Thorium role for user
 * @param {object} errorHandler - error handler function
 * @returns {object} - request response
 */
const createUser = async (name, email, password, role, errorHandler) => {
  const url = '/users/';
  const data = { username: name, email: email, password: password, role: role };
  return client.post(url, data).then((res) => {
    if (res && res.status && res.status == 200 && res.data && res.data.token) {
      return res.data.token;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to register: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to register: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to register: Unknown Error');
    }
    return false;
  });
};

/**
 * Get a User's info by username.
 * @async
 * @function
 * @param {string} username - name of user to get
 * @returns {object} - the user details
 */
const getUser = async (username) => {
  const url = '/users/user/' + username;
  return client.get(url);
};

/**
 * Get a list of users and their details.
 * @param {object} errorHandler - error handler function
 * @param {boolean} details - whether to get user details or just a list of user names
 * @param {string} cursor - the cursor value to continue listing from
 * @param {number} limit - the number of users to return
 * @async
 * @function
 * @returns {object} - list of users
 */
const listUsers = async (errorHandler, details = false, cursor = null, limit = 1000) => {
  let url = '/users/';
  if (details) {
    url += 'details/';
  }
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
      errorHandler(`Failed to list users: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to list users: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to list user: Unknown Error');
    }
    return false;
  });
};

/**
 * Log user out to destroy token.
 * @async
 * @function
 * @returns {object} - request response
 */
const logout = async () => {
  return client.post('/users/logout');
};

/**
 * Update your own user info
 * @async
 * @function
 * @param {object} data - json to patch user with
 * @param {object} errorHandler - error handler function
 * @returns {object} - request response
 */
const updateUser = async (data, errorHandler) => {
  return client.patch(`/users/`, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update user: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to update user: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to update user: Unknown Error');
    }
    return false;
  });
};

/**
 * Update a single user's information with username.
 * @async
 * @function
 * @param {object} data - json to patch user with
 * @param {string} username - username of user to patch
 * @param {object} errorHandler - error handler function
 * @returns {object} - request response
 */
const updateSingleUser = async (data, username, errorHandler) => {
  const url = '/users/user/' + username;
  return client.patch(url, data).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to update user: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to update user: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to update user: Unknown Error');
    }
    return false;
  });
};

/**
 * Get a users name and info from the token.
 * @async
 * @function
 * @returns {object} - user's account info.
 */
const whoami = async () => {
  return client.get('/users/whoami');
};

/**
 * Delete a user by name.
 * @async
 * @function
 * @param {string} user - name of user to delete
 * @param {object} errorHandler - error handler function
 * @returns {object} - request response
 */
const deleteUser = async (user, errorHandler) => {
  const url = '/users/delete/' + user;
  return client.delete(url).then((res) => {
    if (res && res.status && res.status == 204) {
      return true;
      // server responded with unauthorized
    } else if (res && res.status && res.status == 401) {
      errorHandler(`Failed to delete user: Permission Denied`);
      // error is return with message
    } else if (res && res.status && res.data && res.data.error) {
      errorHandler(`${res.data.error}`);
      // error is returned with no message
    } else if (res && res.status) {
      errorHandler(`Failed to delete user: ${res.status}`);
      // no message or error returned
    } else {
      errorHandler('Failed to delete user: Unknown Error');
    }
    return false;
  });
};

export { authUserPass, authUserToken, createUser, getUser, listUsers, logout, updateSingleUser, updateUser, whoami, deleteUser };
