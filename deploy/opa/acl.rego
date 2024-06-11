package app.rbac

import rego.v1

default main := false

main if {
	validate_roles
}

validate_roles if {
	roles := data.metadata_public.project[input.resource]
	some role in roles
	match_url({
		"method": input.method,
		"uri": input.uri,
		"role": role,
	})
}

validate_roles if {
	some type in data.metadata_public
	some scop in type
	some project_id in scop.project
	id := format_int(input.resource, 10)
	id == project_id
	some role in scop.role
	match_url({
		"method": input.method,
		"uri": input.uri,
		"role": role,
	})
}

# check end point right
match_url(var) if {
	some k
	api_attributes = {"post": [
		{"key": "api/iam/projects", "value": ["admin", "owner"]},
		{"key": "api/iam/groups", "value": ["admin", "owner"]},
		{"key": "api/iam/organisation", "value": ["admin", "owner"]},
	]}
	uri_list := api_attributes[var.method]
	some uri_data in uri_list
	glob.match(uri_data.key, ["/"], var.uri)
	some autorized in uri_data.value
	autorized == var.role
}
