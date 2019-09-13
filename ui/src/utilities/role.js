// get the Thorium role for a given user: Admin, Developer, or User
const getThoriumRole = (role) => {
  if (typeof role == 'string') {
    return role;
  } else if (typeof role === 'object' && typeof role !== 'function' && role !== null) {
    if ('Developer' in role) {
      return 'Developer';
    } else {
      return '';
    }
  } else {
    // role might be null or function, either way thats invalid, use an empty string.
    return '';
  }
};

// get the Group role for a given user as a string: Owner, Manager, Monitor, or User
const getGroupRole = (group, user) => {
  if (group.owners.combined.includes(user)) {
    return 'Owner';
  } else if (group.managers.combined.includes(user)) {
    return 'Manager';
  } else if (group.monitors.combined.includes(user)) {
    return 'Monitor';
  } else if (group.users.combined.includes(user)) {
    return 'User';
  } else {
    // this is an error and should never happen
    return '';
  }
};

// check if user is a group admin
const isGroupAdmin = (group, userInfo) => {
  // user is a group admin if they are a manager, owner or Thorium admin
  return group.owners.combined.includes(userInfo.username) ||
    group.managers.combined.includes(userInfo.username) ||
    userInfo.role == 'Admin'
    ? true
    : false;
};

export { getThoriumRole, getGroupRole, isGroupAdmin };
