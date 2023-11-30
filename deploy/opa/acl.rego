package app.rbac

default main := false

# check end point right
match_url {
	some attributes
	api_attributes = {"post": [
		{"key": "api/iam/projects", "value": ["admin", "owner"]},
		{"key": "api/iam/groups", "value": ["admin", "owner"]},
		{"key": "api/iam/organisation", "value": ["admin", "owner"]},
	]}

	uri_list := api_attributes[input.method]
	uri_list[attributes].key == input.uri
	uri_list[attributes].value[_] == input.role
}

validate_roles {
	some roles
	role := data.metadata_public.project[input.resource][roles]
	match_url with input as {
		"method": input.method,
		"uri": input.uri,
		"role": role,
	}
}

validate_roles {
	some type
	some scop
	some project
	project_id := data.metadata_public[type][scop].project[project]
	format_int(project_id, 10) == input.resource
	role := scop.role[_]
	match_url with input as {
		"method": input.method,
		"uri": input.uri,
		"role": role,
	}
}

main {
	validate_roles
}
