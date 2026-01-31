use tree_sitter::{Node, TreeCursor};

/// Visit all nodes in the tree depth-first, calling the visitor for each node
pub fn visit_tree<F>(cursor: &mut TreeCursor, visitor: &mut F)
where
    F: FnMut(Node),
{
    loop {
        let node = cursor.node();
        visitor(node);

        // Recurse into children
        if cursor.goto_first_child() {
            visit_tree(cursor, visitor);
            cursor.goto_parent();
        }

        // Move to next sibling
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Find all nodes of a specific kind in the tree
pub fn find_by_kind<'a>(cursor: &mut TreeCursor<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut results = Vec::new();
    collect_by_kind(cursor, kind, &mut results);
    results
}

fn collect_by_kind<'a>(cursor: &mut TreeCursor<'a>, kind: &str, results: &mut Vec<Node<'a>>) {
    loop {
        let node = cursor.node();
        if node.kind() == kind {
            results.push(node);
        }

        // Recurse into children
        if cursor.goto_first_child() {
            collect_by_kind(cursor, kind, results);
            cursor.goto_parent();
        }

        // Move to next sibling
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Get child node by kind
pub fn get_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

/// Get all children of a specific kind
pub fn get_children_by_kind<'a>(node: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|n| n.kind() == kind)
        .collect()
}
