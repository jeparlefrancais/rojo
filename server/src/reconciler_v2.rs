use std::collections::HashMap;

use rbx_dom_weak::{RbxTree, RbxId, RbxInstanceProperties, RbxValue};

use crate::snapshot_reconciler::RbxSnapshotInstance;

#[derive(Debug, Default)]
pub struct TreeDiff {
    updated: Vec<(RbxId, InstanceDiff)>,
}

#[derive(Debug)]
pub struct InstanceDiff {
    changed_name: Option<String>,
    changed_properties: Vec<String>,
    changed_children: Option<Vec<RbxId>>,
    changed_metadata: Option<()>,
}

#[derive(Debug, Default)]
pub struct TreePatch {
    added: HashMap<RbxId, RbxInstanceProperties>,
    updated: Vec<(RbxId, InstancePatch)>,
}

#[derive(Debug, Default)]
pub struct InstancePatch {
    changed_name: Option<String>,
    changed_properties: HashMap<String, Option<RbxValue>>,
    changed_children: Option<Vec<RbxId>>,
    changed_metadata: Option<()>,
}

impl InstancePatch {
    fn is_empty(&self) -> bool {
        self.changed_name.is_none()
        && self.changed_properties.is_empty()
        && self.changed_children.is_none()
        && self.changed_metadata.is_none()
    }
}

pub fn compute_patch(
    tree: &RbxTree,
    id: RbxId,
    snapshot: &RbxSnapshotInstance<'_>,
) -> TreePatch {
    let mut patch = TreePatch::default();
    compute_patch_core(tree, id, snapshot, &mut patch);
    patch
}

fn compute_patch_core(
    tree: &RbxTree,
    id: RbxId,
    snapshot: &RbxSnapshotInstance<'_>,
    tree_patch: &mut TreePatch,
) {
    let instance = match tree.get_instance(id) {
        Some(instance) => instance,
        None => return,
    };

    let mut instance_patch = InstancePatch::default();

    if instance.class_name != snapshot.class_name {
        panic!("class_name shouldn't change");
    }

    if instance.name != snapshot.name {
        instance_patch.changed_name = Some(instance.name.clone());
    }

    for (key, instance_value) in &instance.properties {
        match snapshot.properties.get(key) {
            Some(snapshot_value) => {
                if snapshot_value != instance_value {
                    instance_patch.changed_properties.insert(key.clone(), Some(snapshot_value.clone()));
                }
            }
            None => {
                instance_patch.changed_properties.insert(key.clone(), None);
            }
        }
    }

    for (key, snapshot_value) in &snapshot.properties {
        if instance_patch.changed_properties.contains_key(key) {
            continue;
        }

        match instance.properties.get(key) {
            Some(instance_value) => {
                if snapshot_value != instance_value {
                    instance_patch.changed_properties.insert(key.clone(), Some(snapshot_value.clone()));
                }
            },
            None => {
                instance_patch.changed_properties.insert(key.clone(), Some(snapshot_value.clone()));
            },
        }
    }

    if !instance_patch.is_empty() {
        tree_patch.updated.push((id, instance_patch));
    }
}

pub fn apply_patch(
    tree: &mut RbxTree,
    mut tree_patch: TreePatch,
) {
    for (id, patch) in tree_patch.updated.into_iter() {
        if let Some(instance) = tree.get_instance_mut(id) {
            for (key, value) in patch.changed_properties.into_iter() {
                match value {
                    Some(value) => instance.properties.insert(key, value),
                    None => instance.properties.remove(&key),
                };
            }

            if let Some(name) = patch.changed_name {
                instance.name = name;
            }
        }

        if let Some(new_children_ids) = patch.changed_children {
            let mut removed_ids = Vec::new();
            let mut added_ids = Vec::new();

            let children_ids = tree.get_instance(id).unwrap().get_children_ids();

            'removed: for child_id in children_ids {
                for new_id in &new_children_ids {
                    if child_id == new_id {
                        continue 'removed;
                    }
                }

                removed_ids.push(*child_id);
            }

            'added: for new_id in &new_children_ids {
                for child_id in children_ids {
                    if child_id == new_id {
                        continue 'added;
                    }
                }

                added_ids.push(*new_id);
            }

            for removed_id in removed_ids.into_iter() {
                tree.remove_instance(removed_id);
            }

            for added_id in added_ids.into_iter() {
                let instance = tree_patch.added.remove(&added_id).unwrap();
                tree.insert_instance(instance, id);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn simple() {
        let tree = RbxTree::new(RbxInstanceProperties {
            name: "DataModel".to_owned(),
            class_name: "DataModel".to_owned(),
            properties: Default::default(),
        });

        let snapshot = RbxSnapshotInstance {
            name: Cow::Borrowed("Not DataModel"),
            class_name: Cow::Borrowed("DataModel"),
            properties: Default::default(),
            children: vec![
                RbxSnapshotInstance {
                    name: Cow::Borrowed("Hi"),
                    class_name: Cow::Borrowed("Folder"),
                    properties: Default::default(),
                    children: Default::default(),
                    metadata: Default::default(),
                },
            ],
            metadata: Default::default(),
        };

        let patch = compute_patch(&tree, tree.get_root_id(), &snapshot);

        println!("{:#?}", patch);
        panic!("fail");
    }
}