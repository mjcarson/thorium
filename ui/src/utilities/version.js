import { getVersion } from '@thorpi';

// pulled from the package.json via a webpack plugin
const LOADED_UI_VERSION = `${process.env.VERSION}`;

/**
 * Checks the UI version and sets flag if updates are available
 * @returns {object} promise for async version check action
 */
async function handleGetUIVersion() {
  const reqVersion = await getVersion(console.log);
  if (reqVersion && 'web_ui' in reqVersion) {
    return reqVersion.web_ui;
  } else {
    return false;
  }
}

// eslint-disable-next-line valid-jsdoc
/**
 * Check if hosted UI version is newer than running version
 * @returns {boolean} whether there is an available UI update
 */
function hasAvailableUIUpdate() {
  const hostedUIVersion = localStorage.getItem('THORIUM_HOSTED_UI_VERSION');
  if (hostedUIVersion && hostedUIVersion != LOADED_UI_VERSION) {
    return true;
  }
  return false;
}

// eslint-disable-next-line valid-jsdoc
/**
 * Force reload of UI to get latest hosted version
 */
function reloadUI() {
  window.location.reload(true);
}

export { hasAvailableUIUpdate, handleGetUIVersion, reloadUI };
