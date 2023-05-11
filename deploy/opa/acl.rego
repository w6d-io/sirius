package app.rbac

import future.keywords.if

default main = false

main if {

	#    # set the role  if the ressource id is present in the role attribute of the user
	role = data.metadata_admin[_][_][input.resource]
	matchUrl with input as {
		"method": input.method,
		"uri": input.uri,
		"role": role,
	}
}

# check end point right
matchUrl if {
	some k
	api_attributes = {"post": [
		{"key": "api/iam/project/", "value": ["admin", "owner"]},
		{"key": "api/iam/scope/", "value": ["admin", "owner"]},
		{"key": "api/iam/organisation/", "value": ["admin", "owner"]},

	]}

	uri_list := api_attributes[input.method]
	uri_list[k].key == input.uri
	uri_list[k].value[_] == input.role
}
