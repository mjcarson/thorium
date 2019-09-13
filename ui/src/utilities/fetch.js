import { getImage, listGroups, listImages } from '@thorpi';

/**
 * Get a single image spec from the Thorium API
 * @param {object} image image object containing a name and group field
 * @param {Function} setImage function to set the updated image
 * @param {Function} setLoading function to set whether currently making API request
 * @returns {Promise} a promise for the request action
 */
async function fetchSingleImage(image, setImage, setLoading) {
  setLoading(true);
  const reqImage = await getImage(image.group, image.name);
  if (reqImage) {
    setImage(reqImage.data);
  }
  setLoading(false);
}

/**
 * Get a list of images from a users accessible groups
 * @param {object} groups A list of string groups a user can see
 * @param {Function} setImages usestate seter for image details
 * @param {boolean} cancelUpdate boolean indicating do not update if component is mounted
 * @param {Function} setError an auth hook for validating cookies when 401s are returned
 * @param {Function} setLoading is the fetch function loading new data
 * @param {boolean} details whether to fetch image details or just names
 * @returns {object} async promise for request
 */
async function fetchImages(groups, setImages, cancelUpdate, setError, setLoading, details = false) {
  if (typeof setLoading == 'function') setLoading(true);

  const images = [];
  if (groups && Array.isArray(groups) && groups.length) {
    // Get image details for each group
    for (const group of groups) {
      const reqImages = await listImages(group, setError, details, null, 1000);
      if (reqImages) {
        // request successful, pass back image details in a list or just a list of names
        if (details) images.push(...reqImages);
        else images.push(...reqImages.names);
      } else {
        // when request failed, reset images to empty
        setImages([]);
      }
    }
    // don't update if component isn't mounted
    // this occurs on redirect to login page
    if (!cancelUpdate) {
      if (details) {
        setImages(images.sort((a, b) => (a.group + a.name).localeCompare(b.group + b.name)));
      } else {
        setImages(images);
      }
    }
  }
  if (typeof setLoading == 'function') setLoading(false);
}

// get a list of groups to get group roles
/**
 * Get details for a users groups
 * @param {Function} setGroups useState setter for group details
 * @param {object} checkCookie an auth hook for validating cookies when 401s are returned
 * @param {boolean} setLoading whether there is currently an outstanding API request
 * @param {boolean} details whether to return group details in object or list of group names
 * @param {string} returnType the type of groups info to return, options are Array or Object
 * @returns {object} async promise for request
 */
async function fetchGroups(setGroups, checkCookie, setLoading, details = false, returnType = 'Object') {
  // set loading when interacting w/ API
  if (typeof setLoading == 'function') setLoading(true);

  const reqGroups = await listGroups(checkCookie, details);
  if (reqGroups) {
    if (details) {
      if (returnType == 'Object') {
        const allGroups = {};
        for (const group of reqGroups) {
          allGroups[group.name] = group;
        }
        setGroups(allGroups);
      } else if (returnType == 'Array') {
        setGroups(reqGroups.sort((a, b) => a.name.localeCompare(b.name)));
      }
      // when listing groups without details, there is an array in the `names` key
    } else if (reqGroups.names) {
      const allGroups = reqGroups.names.sort();
      setGroups(allGroups);
    } else {
      // details is false, but groups response wasn't valid
      setGroups([]);
    }
  } else {
    // request failed for some reason, reset groups to falsey
    setGroups(null);
  }

  // done interacting w/ API, set loading to false
  if (typeof setLoading == 'function') setLoading(false);
}

export { fetchGroups, fetchImages, fetchSingleImage };
