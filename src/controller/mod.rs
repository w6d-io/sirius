pub mod list;
pub mod sync;
pub mod update;

//identity meta:
//
//project : project-id(uint) [role1, role2]
//
//group : uuid-000-001 {
//  name: toto,
//  project(vec[project-id]): [1234, 5678]
//  role: [admin,maintainer]
//}

//organisation : uuid-000-001 {
//  name: toto,
//  project(vec[project-id]): [1234, 5678]
//  role: [admin,maintainer]
//}
