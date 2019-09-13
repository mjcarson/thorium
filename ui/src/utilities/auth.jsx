/* eslint-disable valid-jsdoc */
/* eslint-disable jsdoc/require-param */
/* eslint-disable jsdoc/require-returns */
/* eslint-disable require-jsdoc */
/* eslint-disable jsdoc/require-jsdoc */
import React, { createContext, useContext, useEffect, useState } from 'react';
import { Navigate } from 'react-router-dom';
import Cookies from 'js-cookie';
import { authUserPass, createUser, logout, whoami } from '@thorpi';

// auth context to store info about auth state across app
const authContext = createContext();

/*
 * Thorium auth hooks for login, logout and token revocation
 */
function useAuthProvider() {
  const [userInfo, setUserInfo] = useState({});
  const [token, setToken] = useState(Cookies.get('THORIUM_TOKEN'));
  // set time of last userInfo update
  const [lastUpdateDate, setLastUpdateDate] = useState(Date.now());
  // options for set/get of a secure cookie
  const cookieOptions = {
    expires: 7,
    path: '/',
    secure: true,
    sameSite: 'strict',
    domain: location.hostname,
  };
  const getUserInfo = async () => {
    // get user details
    if (token != undefined) {
      whoami().then((response) => {
        if (response && response.status && response.status == 200) {
          setUserInfo(response.data);
          setToken(response.data.token);
          setLastUpdateDate(new Date());
        } else if (response && response.status && response.status == 401) {
          Cookies.remove('THORIUM_TOKEN', cookieOptions);
          setToken('');
        }
      });
    }
  };

  useEffect(() => {
    // update theme if userInfo changes
    if (userInfo && userInfo['settings'] && userInfo.settings['theme']) {
      const root = document.getElementById('root');
      // automatic theme will use browser defaults
      if (userInfo.settings['theme'] == 'Automatic') {
        if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
          root.setAttribute('theme', 'Dark');
        } else {
          root.setAttribute('theme', 'Light');
        }
      } else {
        root.setAttribute('theme', userInfo.settings.theme);
      }
    }
  }, [userInfo]);

  useEffect(() => {
    getUserInfo();
    // need to run whoami to get user info after login
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  return {
    userInfo,
    token,
    // verify userInfo is not out-of-date
    async refreshUserInfo(force = false) {
      // check if userInfo is fresher than 60 seconds (60k msec)
      if (force || Date.now() - lastUpdateDate > 60000) {
        // refresh user info
        getUserInfo();
      }
      return;
    },
    // validate cookie with request to Thorium whoami route
    // a 401 response will clear cookie
    checkCookie() {
      return new Promise((resolve) => {
        if (Cookies.get('THORIUM_TOKEN')) {
          whoami().then((response) => {
            // update cookie and session user info on success
            if (response && response.status && response.status == 200) {
              setUserInfo(response.data);
              setToken(response.data.token);
              setLastUpdateDate(Date.now());
              resolve(response.data);
              // clear cookie on unauthorized response
            } else if (response && response.status && response.status == 401) {
              Cookies.remove('THORIUM_TOKEN', cookieOptions);
              setUserInfo({});
              setToken(undefined);
              resolve({});
            }
          });
        }
      });
    },
    // login via password to get Thorium token
    login(username, password, setLoginError) {
      return new Promise((res) => {
        authUserPass(username, password, setLoginError).then((token) => {
          if (token) {
            // set cookie with name THORIUM_TOKEN
            Cookies.set('THORIUM_TOKEN', token, cookieOptions);
            // set user's Thorium token
            setToken(token);
          }
          res(token);
        });
      });
    },
    // remove token and clear user info on logout
    logout() {
      return new Promise((res) => {
        setToken(undefined);
        setUserInfo({});
        Cookies.remove('THORIUM_TOKEN', cookieOptions);
        res();
      });
    },
    // register with Thorium
    register(username, password, setRegError, email = 'thorium@sandia.gov', role = 'User') {
      return new Promise((res) => {
        createUser(username, email, password, role, setRegError).then((thoriumToken) => {
          if (thoriumToken) {
            // set cookie with name THORIUM_TOKEN
            Cookies.set('THORIUM_TOKEN', thoriumToken, cookieOptions);
            // set user's Thorium token
            setToken(thoriumToken);
            res(true);
          } else {
            res(false);
          }
        });
      });
    },
    // revoke token and clear cookie user info from session
    revoke() {
      return new Promise((res) => {
        /**
         * Submits a token revocation request to the Thorium API
         * @returns {object} promise for async revocation action
         */
        async function handleRevoke() {
          const response = await logout();
          if (response && response.status && response.status == 200) {
            res();
          }
        }
        handleRevoke().then(() => {
          setToken(undefined);
          setUserInfo({});
          Cookies.remove('THORIUM_TOKEN', cookieOptions);
          res();
        });
      });
    },
    // logout of any current session and impersonate a user
    async impersonate(userToken) {
      Cookies.set('THORIUM_TOKEN', userToken, cookieOptions);
      setToken(userToken);
    },
  };
}

/**
 * Wrap application in a shared auth provider
 */
function AuthProvider({ children }) {
  const auth = useAuthProvider();
  return <authContext.Provider value={auth}>{children}</authContext.Provider>;
}

const useAuth = () => {
  return useContext(authContext);
};

/**
 * Validate that user is logged and redirect on validation failure
 */
function RequireAuth({ children }) {
  const { token } = useAuth();
  // token must be set and cookie must still be set
  // cookie gets cleared when it is expired

  return token != undefined && Cookies.get('THORIUM_TOKEN') != undefined ? (
    children
  ) : (
    <Navigate to="/auth" replace state={{ path: location.pathname + location.search + location.hash }} />
  );
}

/**
 * Validate user's Thorium role is admin and redirects on authorization failure
 */
function RequireAdmin({ children }) {
  const { userInfo } = useAuth();
  return userInfo && userInfo.role === 'Admin' ? (
    children
  ) : (
    <Navigate replace state={{ path: location.pathname + location.search + location.hash }} />
  );
}

export { useAuth, RequireAuth, RequireAdmin, AuthProvider };
