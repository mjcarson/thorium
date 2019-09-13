/**
 * Get the API URL path as a string
 * @returns {string} API URL path
 */
function getApiUrl() {
  if (window.location.hostname == 'localhost') {
    return process.env.REACT_APP_API_URL;
  } else {
    return `${window.location.protocol}//${window.location.hostname}/api`;
  }
}

// Update url hash location with section string
// append -tab to end of sections, results get an optional -subsection
const updateURLSection = (section, subsection) => {
  const updatedHash = subsection ? `#${section}-${subsection}` : `#${section}`;
  window.location.hash = updatedHash;
};

export { getApiUrl, updateURLSection };
