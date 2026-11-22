//! Alberatura dei file elaborati di una categoria (copiati, eliminati, ...): i percorsi vengono inseriti
//! già in ordine, prima le cartelle e poi i file, in ordine alfabetico. La GUI mostra le righe "appiattite"
//! dei soli nodi visibili (cartelle espanse).

use std::cmp::Ordering;

use super::TreeRow;
use super::format;

/// Fino a questo numero di elementi l'albero viene mostrato già tutto espanso.
const EXPAND_ALL_LIMIT: usize = 300;

#[derive(Debug, Default)]
struct Node {
    name: String,
    directory: bool,
    whole_folder: bool, // cartella eliminata o esclusa per intero: il contenuto non è elencato
    count: usize,
    expanded: bool,
    children: Vec<Node>,
}

impl Node {
    fn new(name: &str, directory: bool) -> Self {
        Node { name: name.to_string(), directory, ..Default::default() }
    }

    /// Cartelle prima dei file, poi ordine alfabetico senza distinguere maiuscole/minuscole.
    fn compare(&self, name: &str, directory: bool) -> Ordering {
        directory
            .cmp(&self.directory)
            .then_with(|| self.name.to_lowercase().cmp(&name.to_lowercase()))
            .then_with(|| self.name.as_str().cmp(name))
    }

    fn set_expanded_recursive(&mut self, expanded: bool) {
        self.expanded = expanded;
        for child in &mut self.children {
            child.set_expanded_recursive(expanded);
        }
    }
}

#[derive(Debug)]
pub struct FileTree {
    root: Node,
}

impl Default for FileTree {
    fn default() -> Self {
        let mut root = Node::new("", true);
        root.expanded = true;
        FileTree { root }
    }
}

impl FileTree {
    pub fn count(&self) -> usize {
        self.root.count
    }

    /// Percorso relativo con "/" come separatore; le cartelle intere terminano con "/".
    pub fn insert(&mut self, path: &str) {
        let whole_folder = path.ends_with('/');
        let names: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let mut node = &mut self.root;
        node.count += 1;
        for (i, name) in names.iter().enumerate() {
            let last = i == names.len() - 1;
            let directory = !last || whole_folder;
            let index = match node.children.binary_search_by(|c| c.compare(name, directory)) {
                Ok(index) => index,
                Err(index) => {
                    node.children.insert(index, Node::new(name, directory));
                    index
                }
            };
            node = &mut node.children[index];
            node.count += 1;
            node.whole_folder |= last && whole_folder;
        }
    }

    /// Righe visibili e, per ciascuna, la posizione del nodo (indici dei figli dalla radice).
    pub fn rows(&self) -> (Vec<TreeRow>, Vec<Vec<usize>>) {
        let mut rows = Vec::new();
        let mut paths = Vec::new();
        let mut current = Vec::new();
        collect(&self.root, 0, &mut current, &mut rows, &mut paths);
        (rows, paths)
    }

    pub fn toggle(&mut self, path: &[usize]) {
        let mut node = &mut self.root;
        for &index in path {
            match node.children.get_mut(index) {
                Some(child) => node = child,
                None => return,
            }
        }
        node.expanded = !node.expanded;
    }

    pub fn set_all_expanded(&mut self, expanded: bool) {
        for child in &mut self.root.children {
            child.set_expanded_recursive(expanded);
        }
    }

    /// Tutto espanso se l'albero è piccolo, altrimenti solo le catene di cartelle con un unico figlio (es. src/main/java).
    pub fn expand_default(&mut self) {
        if self.root.count <= EXPAND_ALL_LIMIT {
            self.set_all_expanded(true);
            return;
        }
        self.set_all_expanded(false);
        let mut node = &mut self.root;
        while node.children.len() == 1 && !node.children[0].children.is_empty() {
            node = &mut node.children[0];
            node.expanded = true;
        }
    }
}

fn collect(node: &Node, depth: i32, current: &mut Vec<usize>, rows: &mut Vec<TreeRow>, paths: &mut Vec<Vec<usize>>) {
    for (index, child) in node.children.iter().enumerate() {
        current.push(index);
        let detail = if !child.directory {
            String::new()
        } else if child.whole_folder {
            "(cartella intera)".to_string()
        } else {
            format::count(child.count)
        };
        rows.push(TreeRow {
            depth,
            name: child.name.as_str().into(),
            directory: child.directory,
            expanded: child.expanded,
            expandable: !child.children.is_empty(),
            detail: detail.into(),
        });
        paths.push(current.clone());
        if child.expanded {
            collect(child, depth + 1, current, rows, paths);
        }
        current.pop();
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn names(tree: &FileTree) -> Vec<String> {
        tree.rows().0.iter().map(|r| format!("{}{}", "  ".repeat(r.depth as usize), r.name)).collect()
    }

    #[test]
    fn folders_first_then_alphabetical() {
        let mut tree = FileTree::default();
        for path in ["b.txt", "src/Main.java", "A.txt", "lib/", "src/a/x.java"] {
            tree.insert(path);
        }
        tree.expand_default();
        assert_eq!(names(&tree), vec!["lib", "src", "  a", "    x.java", "  Main.java", "A.txt", "b.txt"]);
        assert_eq!(tree.count(), 5);
        let rows = tree.rows().0;
        assert_eq!(rows[0].detail, "(cartella intera)");
        assert_eq!(rows[1].detail, "2");
    }

    #[test]
    fn toggle_and_large_trees_collapse() {
        let mut tree = FileTree::default();
        for i in 0..400 {
            tree.insert(&format!("src/main/java/pkg{}/F{i}.java", i % 4));
        }
        tree.expand_default();
        // catena con un solo figlio espansa, il resto chiuso
        assert_eq!(names(&tree), vec!["src", "  main", "    java", "      pkg0", "      pkg1", "      pkg2", "      pkg3"]);
        let (_, paths) = tree.rows();
        tree.toggle(&paths[3]);
        assert_eq!(tree.rows().0.len(), 7 + 100);
    }

}
