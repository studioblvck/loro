use rle::{RleTree, Sliceable};
use smallvec::SmallVec;

use crate::{
    container::{list::list_op::ListOp, Container, ContainerID, ContainerType},
    dag::{Dag, DagUtils},
    id::ID,
    log_store::LogStoreWeakRef,
    op::{InsertContent, Op, OpContent, OpProxy},
    smstring::SmString,
    span::{HasIdSpan, IdSpan},
    value::LoroValue,
    LogStore,
};

use super::{
    string_pool::StringPool,
    text_content::{ListSlice, ListSliceTreeTrait},
    tracker::{Effect, Tracker},
};

#[derive(Clone, Debug)]
struct DagNode {
    id: IdSpan,
    deps: SmallVec<[ID; 2]>,
}

#[derive(Debug)]
pub struct TextContainer {
    id: ContainerID,
    log_store: LogStoreWeakRef,
    state: RleTree<ListSlice, ListSliceTreeTrait>,
    raw_str: StringPool,
    tracker: Tracker,
    state_cache: LoroValue,
}

impl TextContainer {
    pub fn new(id: ContainerID, log_store: LogStoreWeakRef) -> Self {
        Self {
            id,
            log_store,
            raw_str: StringPool::default(),
            tracker: Tracker::new(Default::default()),
            state_cache: LoroValue::Null,
            state: Default::default(),
        }
    }

    pub fn insert(&mut self, pos: usize, text: &str) -> Option<ID> {
        let id = if let Ok(mut store) = self.log_store.upgrade().unwrap().write() {
            let id = store.next_id();
            let slice = ListSlice::from_range(self.raw_str.alloc(text));
            self.state.insert(pos, slice.clone());
            let op = Op::new(
                id,
                OpContent::Normal {
                    content: InsertContent::List(ListOp::Insert { slice, pos }),
                },
                self.id.clone(),
            );
            store.append_local_ops(vec![op]);
            id
        } else {
            unimplemented!()
        };

        Some(id)
    }

    pub fn delete(&mut self, pos: usize, len: usize) -> Option<ID> {
        let id = if let Ok(mut store) = self.log_store.upgrade().unwrap().write() {
            let id = store.next_id();
            let op = Op::new(
                id,
                OpContent::Normal {
                    content: InsertContent::List(ListOp::Delete { len, pos }),
                },
                self.id.clone(),
            );

            store.append_local_ops(vec![op]);
            self.state.delete_range(Some(pos), Some(pos + len));
            id
        } else {
            unimplemented!()
        };

        Some(id)
    }

    pub fn text_len(&self) -> usize {
        self.state.len()
    }
}

impl Container for TextContainer {
    fn id(&self) -> &ContainerID {
        &self.id
    }

    fn type_(&self) -> ContainerType {
        ContainerType::Text
    }

    // TODO: move main logic to tracker module
    // TODO: we don't need op proxy, only ids are enough
    fn apply(&mut self, op: &OpProxy, store: &LogStore) {
        let new_op_id = op.id_last();
        // TODO: may reduce following two into one op
        let common = store.find_common_ancestor(&[new_op_id], store.frontier());
        let path_to_store_head = store.find_path(&common, store.frontier());
        let mut common_vv = store.vv();
        common_vv.retreat(&path_to_store_head.right);
        let mut latest_head: SmallVec<[ID; 2]> = store.frontier().into();
        latest_head.push(new_op_id);
        if common.is_empty() || !common.iter().all(|x| self.tracker.contains(*x)) {
            // stage 1
            self.tracker = Tracker::new(common_vv);
            let path = store.find_path(&common, &latest_head);
            for iter in store.iter_partial(&common, path.right) {
                self.tracker.retreat(&iter.retreat);
                self.tracker.forward(&iter.forward);
                // TODO: avoid this clone
                let change = iter
                    .data
                    .slice(iter.slice.start as usize, iter.slice.end as usize);
                for op in change.ops.iter() {
                    if op.container == self.id {
                        // TODO: convert op to local
                        self.tracker.apply(op.id, &op.content)
                    }
                }
            }
        }

        // stage 2
        let path = store.find_path(&latest_head, store.frontier());
        self.tracker.retreat(&path.left);
        for effect in self.tracker.iter_effects(path.left) {
            match effect {
                Effect::Del { pos, len } => self.state.delete_range(Some(pos), Some(pos + len)),
                Effect::Ins { pos, content } => {
                    let content = store.get_op_content(content).unwrap();
                    let list_content = content.as_normal().unwrap().as_list().unwrap();
                    let insert_content = list_content.as_insert().unwrap().0;
                    self.state.insert(pos, insert_content.clone());
                }
            }
        }
    }

    fn checkout_version(&mut self, _vv: &crate::VersionVector) {
        todo!()
    }

    fn get_value(&mut self) -> &LoroValue {
        let mut ans_str = SmString::new();
        for v in self.state.iter() {
            let content = v.as_ref();
            match content {
                ListSlice::Slice(range) => ans_str.push_str(&self.raw_str.get_str(range)),
                ListSlice::RawStr(raw) => ans_str.push_str(raw),
                _ => unreachable!(),
            }
        }

        self.state_cache = LoroValue::String(ans_str);
        &self.state_cache
    }

    fn to_export(&self, op: &mut Op) {
        if let Some((slice, _pos)) = op
            .content
            .as_normal_mut()
            .and_then(|c| c.as_list_mut())
            .and_then(|x| x.as_insert_mut())
        {
            let change = if let ListSlice::Slice(ranges) = slice {
                Some(self.raw_str.get_str(ranges))
            } else {
                None
            };

            if let Some(change) = change {
                *slice = ListSlice::RawStr(change);
            }
        }
    }
}
