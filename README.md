# sirius

## SIRIUS
Sirius is an api that used to list and modify kratos identity.

### routes

``/api/iam/project``:
This route is used to add or list projects in the identity 
send a GET request with a kratos cookie to list the projects.
send a POST request with a kratos cookie and the projects to add.

``/api/iam/group``:
This route is used to list groups in an identity or add projects to a group
send a GET request with a kratos cookie to list the groups in the identity.
send a POST request with a kratos cookie and the projects or users to add.

``/api/iam/organisation``:
This route is used to list organisations in an identity or add  to a group
send a GET request with a kratos cookie to list the organizations in the identity.
send a POST request with a kratos cookie and the projects, groups or users to add.

In all the POST case you must use this json payload:
```json
{
    "id" = "string"
    "resource_type" = "string"
    "ressource_id" = "string"
    "value" = json value
}
```
> The id field represent the id of the identity to modify(can be anid or an email
depending on the route).


> The resource_type field represent the type of permission to modify:
> - user
> - group
> - organization

> - The ressource id represent the id of resource to modify.

> - the value field represent the data to modify the identity with
