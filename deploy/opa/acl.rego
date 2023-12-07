package app.rbac

import future.keywords.in

default main := false

# check end point right
match_url {
	some k
	api_attributes = {"post": [
		{"key": "api/iam/projects", "value": ["admin", "owner"]},
		{"key": "api/iam/groups", "value": ["admin", "owner"]},
		{"key": "api/iam/organisation", "value": ["admin", "owner"]},
	]}

	uri_list := api_attributes[input.method]
	uri_list[k].key == input.uri
	uri_list[k].value[_] == input.role
}

validate_roles {
	roles := data.metadata_public.project[input.resource]
	some role in roles
	match_url with input as {
		"method": input.method,
		"uri": input.uri,
		"role": role,
	}
}

validate_roles {
	some type in data.metadata_public
	some scop in type
	some project_id in scop.project
	id := format_int(input.resource, 10)
	id == project_id
	some role in scop.role
	match_url with input as {
		"method": input.method,
		"uri": input.uri,
		"role": role,
	}
}

main {
	validate_roles
}
