use std::{borrow::Cow, collections::HashMap, path::Path};

use memofs::{IoResultExt, Vfs};
use rbx_reflection::try_resolve_value;

use crate::{
    project::{Project, ProjectNode},
    snapshot::{
        InstanceContext, InstanceMetadata, InstanceSnapshot, InstigatingSource, PathIgnoreRule,
    },
};

use super::{
    error::SnapshotError,
    middleware::{SnapshotInstanceResult, SnapshotMiddleware},
    snapshot_from_vfs,
};

/// Handles snapshots for:
/// * Files ending in `.project.json`
/// * Folders containing a file named `default.project.json`
pub struct SnapshotProject;

impl SnapshotMiddleware for SnapshotProject {
    fn from_vfs(context: &InstanceContext, vfs: &Vfs, path: &Path) -> SnapshotInstanceResult {
        let meta = vfs.metadata(path)?;

        if meta.is_dir() {
            let project_path = path.join("default.project.json");

            match vfs.metadata(&project_path).with_not_found()? {
                // TODO: Do we need to muck with the relevant paths if we're a
                // project file within a folder? Should the folder path be the
                // relevant path instead of the project file path?
                Some(_meta) => return SnapshotProject::from_vfs(context, vfs, &project_path),
                None => return Ok(None),
            }
        }

        if !path.to_string_lossy().ends_with(".project.json") {
            // This isn't a project file, so it's not our job.
            return Ok(None);
        }

        let project = Project::load_from_slice(&vfs.read(path)?, path)
            .map_err(|err| SnapshotError::malformed_project(err, path))?;

        let mut context = context.clone();

        let rules = project.glob_ignore_paths.iter().map(|glob| PathIgnoreRule {
            glob: glob.clone(),
            base_path: project.folder_location().to_path_buf(),
        });

        context.add_path_ignore_rules(rules);

        // Snapshotting a project should always return an instance, so this
        // unwrap is safe.
        let mut snapshot = snapshot_project_node(
            &context,
            project.folder_location(),
            &project.name,
            &project.tree,
            vfs,
        )?
        .unwrap();

        // Setting the instigating source to the project file path is a little
        // coarse.
        //
        // Ideally, we'd only snapshot the project file if the project file
        // actually changed. Because Rojo only has the concept of one
        // relevant path -> snapshot path mapping per instance, we pick the more
        // conservative approach of snapshotting the project file if any
        // relevant paths changed.
        snapshot.metadata.instigating_source = Some(path.to_path_buf().into());

        // Mark this snapshot (the root node of the project file) as being
        // related to the project file.
        //
        // We SHOULD NOT mark the project file as a relevant path for any
        // nodes that aren't roots. They'll be updated as part of the project
        // file being updated.
        snapshot.metadata.relevant_paths.push(path.to_path_buf());

        Ok(Some(snapshot))
    }
}

pub fn snapshot_project_node(
    context: &InstanceContext,
    project_folder: &Path,
    instance_name: &str,
    node: &ProjectNode,
    vfs: &Vfs,
) -> SnapshotInstanceResult {
    let name = Cow::Owned(instance_name.to_owned());
    let mut class_name = node
        .class_name
        .as_ref()
        .map(|name| Cow::Owned(name.clone()));
    let mut properties = HashMap::new();
    let mut children = Vec::new();
    let mut metadata = InstanceMetadata::default();

    if let Some(path) = &node.path {
        // If the path specified in the project is relative, we assume it's
        // relative to the folder that the project is in, project_folder.
        let path = if path.is_relative() {
            Cow::Owned(project_folder.join(path))
        } else {
            Cow::Borrowed(path)
        };

        if let Some(snapshot) = snapshot_from_vfs(context, vfs, &path)? {
            // If a class name was already specified, then it'll override the
            // class name of this snapshot ONLY if it's a Folder.
            //
            // This restriction is in place to prevent applying properties to
            // instances that don't make sense. The primary use-case for using
            // $className and $path at the same time is to use a directory as a
            // service in a place file.
            class_name = match class_name {
                Some(class_name) => {
                    if snapshot.class_name == "Folder" {
                        Some(class_name)
                    } else {
                        // TODO: Turn this into an error object.
                        panic!("If $className and $path are specified, $path must yield an instance of class Folder");
                    }
                }
                None => Some(snapshot.class_name),
            };

            // Properties from the snapshot are pulled in unchanged, and
            // overridden by properties set on the project node.
            properties.reserve(snapshot.properties.len());
            for (key, value) in snapshot.properties.into_iter() {
                properties.insert(key, value);
            }

            // The snapshot's children will be merged with the children defined
            // in the project node, if there are any.
            children.reserve(snapshot.children.len());
            for child in snapshot.children.into_iter() {
                children.push(child);
            }

            // Take the snapshot's metadata as-is, which will be mutated later
            // on.
            metadata = snapshot.metadata;
        } else {
            // TODO: Should this issue an error instead?
            log::warn!(
                "$path referred to a path that could not be turned into an instance by Rojo"
            );
        }
    }

    let class_name = class_name
        // TODO: Turn this into an error object.
        .expect("$className or $path must be specified");

    for (child_name, child_project_node) in &node.children {
        if let Some(child) =
            snapshot_project_node(context, project_folder, child_name, child_project_node, vfs)?
        {
            children.push(child);
        }
    }

    for (key, value) in &node.properties {
        let resolved_value = try_resolve_value(&class_name, key, value)
            .expect("TODO: Properly handle value resolution errors");

        properties.insert(key.clone(), resolved_value);
    }

    // If the user specified $ignoreUnknownInstances, overwrite the existing
    // value.
    //
    // If the user didn't specify it AND $path was not specified (meaning
    // there's no existing value we'd be stepping on from a project file or meta
    // file), set it to true.
    if let Some(ignore) = node.ignore_unknown_instances {
        metadata.ignore_unknown_instances = ignore;
    } else if node.path.is_none() {
        // TODO: Introduce a strict mode where $ignoreUnknownInstances is never
        // set implicitly.
        metadata.ignore_unknown_instances = true;
    }

    metadata.instigating_source = Some(InstigatingSource::ProjectNode(
        project_folder.to_path_buf(),
        instance_name.to_string(),
        node.clone(),
    ));

    Ok(Some(InstanceSnapshot {
        snapshot_id: None,
        name,
        class_name,
        properties,
        children,
        metadata,
    }))
}

#[cfg(test)]
mod test {
    use super::*;

    use maplit::hashmap;
    use memofs::{InMemoryFs, VfsSnapshot};

    #[test]
    fn project_from_folder() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "indirect-project",
                        "tree": {
                            "$className": "Folder"
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_from_direct_file() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "hello.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "direct-project",
                        "tree": {
                            "$className": "Model"
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot = SnapshotProject::from_vfs(
            &InstanceContext::default(),
            &mut vfs,
            Path::new("/foo/hello.project.json"),
        )
        .expect("snapshot error")
        .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_resolved_properties() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "resolved-properties",
                        "tree": {
                            "$className": "StringValue",
                            "$properties": {
                                "Value": {
                                    "Type": "String",
                                    "Value": "Hello, world!"
                                }
                            }
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_unresolved_properties() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "unresolved-properties",
                        "tree": {
                            "$className": "StringValue",
                            "$properties": {
                                "Value": "Hi!"
                            }
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_children() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "children",
                        "tree": {
                            "$className": "Folder",

                            "Child": {
                                "$className": "Model"
                            }
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_path_to_txt() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "path-project",
                        "tree": {
                            "$path": "other.txt"
                        }
                    }
                "#),
                "other.txt" => VfsSnapshot::file("Hello, world!"),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_path_to_project() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "path-project",
                        "tree": {
                            "$path": "other.project.json"
                        }
                    }
                "#),
                "other.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "other-project",
                        "tree": {
                            "$className": "Model"
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    #[test]
    fn project_with_path_to_project_with_children() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "path-child-project",
                        "tree": {
                            "$path": "other.project.json"
                        }
                    }
                "#),
                "other.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "other-project",
                        "tree": {
                            "$className": "Folder",

                            "SomeChild": {
                                "$className": "Model"
                            }
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }

    /// Ensures that if a property is defined both in the resulting instance
    /// from $path and also in $properties, that the $properties value takes
    /// precedence.
    #[test]
    fn project_path_property_overrides() {
        let _ = env_logger::try_init();

        let mut imfs = InMemoryFs::new();
        imfs.load_snapshot(
            "/foo",
            VfsSnapshot::dir(hashmap! {
                "default.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "path-property-override",
                        "tree": {
                            "$path": "other.project.json",
                            "$properties": {
                                "Value": "Changed"
                            }
                        }
                    }
                "#),
                "other.project.json" => VfsSnapshot::file(r#"
                    {
                        "name": "other-project",
                        "tree": {
                            "$className": "StringValue",
                            "$properties": {
                                "Value": "Original"
                            }
                        }
                    }
                "#),
            }),
        )
        .unwrap();

        let mut vfs = Vfs::new(imfs);

        let instance_snapshot =
            SnapshotProject::from_vfs(&InstanceContext::default(), &mut vfs, Path::new("/foo"))
                .expect("snapshot error")
                .expect("snapshot returned no instances");

        insta::assert_yaml_snapshot!(instance_snapshot);
    }
}
