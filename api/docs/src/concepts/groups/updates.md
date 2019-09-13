# Updating Groups

Owners and managers are able to update their groups to differing degrees.

| role | abilities |
| --- | ---------- |
| owners | delete entire group and modify roles |
| managers | modify non-owner roles |

Owners are able to delete the entire group and add or remove other owners.
They cannot however remove themselves as an owner. As this could lead to a group
being orphaned with no owners. Maintainers can modify user roles except for owner
roles. This means managers can create other managers but cannot add/remove owners.

This can be done by PATCHing to
```
<api_url>/groups/:group
```
The body of the request will contain the users you want to add and remove from roles.
If you give and remove the same role from the same user they will be removed from the
group entirely. All fields are optional and remember you cannot mix LDAP and manual
Thorium permissions for the same role.

```json
{
  "add_owners": ["bob"],
  "remove_owners": ["todd"],
  "add_managers": ["todd", "sue"],
  "remove_managers": ["bob"],
  "add_users": ["mary"],
  "remove_users": ["chuck"],
  "add_monitors": ["report_bot"],
  "remove_monitors": ["old_bot"],
  "add_ldap_owners": ["ldap-owners"],
  "remove_ldap_owners": ["old-ldap-owners"],
  "add_ldap_managers": ["ldap-managers"],
  "remove_ldap_managers": ["old-ldap-managers"],
  "add_ldap_users": ["ldap-users"],
  "remove_ldap_users": ["old-ldap-users"],
  "add_ldap_monitors": ["ldap-monitors"],
  "remove_ldap_monitors": ["old-ldap-monitors"]
}
```
