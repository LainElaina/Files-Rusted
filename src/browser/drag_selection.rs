use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl DragPoint {
    pub(super) fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DragRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl DragRect {
    pub(super) fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub(super) fn from_points(start: DragPoint, end: DragPoint) -> Self {
        let left = start.x.min(end.x);
        let top = start.y.min(end.y);
        let right = start.x.max(end.x);
        let bottom = start.y.max(end.y);
        Self::new(left, top, right - left, bottom - top)
    }

    pub(super) fn intersects(self, other: Self) -> bool {
        let self_right = self.x + self.width;
        let self_bottom = self.y + self.height;
        let other_right = other.x + other.width;
        let other_bottom = other.y + other.height;

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VisibleItemLayout {
    pub(super) path: PathBuf,
    pub(super) rect: DragRect,
}

#[derive(Clone, Debug)]
pub(super) struct DragSelectionSnapshot {
    pub(super) selected: Vec<PathBuf>,
    pub(super) primary: Option<PathBuf>,
    pub(super) anchor: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DragSelectionResult {
    pub(super) selected: Vec<PathBuf>,
    pub(super) primary: Option<PathBuf>,
    pub(super) anchor: Option<PathBuf>,
    pub(super) rect: Option<DragRect>,
    pub(super) active: bool,
}

#[derive(Clone, Debug)]
pub(super) struct DragSelectionSession {
    start: DragPoint,
    pub(super) control: bool,
    pub(super) baseline: DragSelectionSnapshot,
}

impl DragSelectionSession {
    pub(super) fn begin(start: DragPoint, control: bool, baseline: DragSelectionSnapshot) -> Self {
        Self {
            start,
            control,
            baseline,
        }
    }

    pub(super) fn selection_for(
        &self,
        current: DragPoint,
        layouts: &[VisibleItemLayout],
        threshold: f32,
    ) -> DragSelectionResult {
        if drag_distance(self.start, current) < threshold {
            return DragSelectionResult {
                selected: self.baseline.selected.clone(),
                primary: self.baseline.primary.clone(),
                anchor: self.baseline.anchor.clone(),
                rect: None,
                active: false,
            };
        }

        let rect = DragRect::from_points(self.start, current);
        let hit_paths = layouts
            .iter()
            .filter(|layout| rect.intersects(layout.rect))
            .map(|layout| layout.path.clone())
            .collect::<Vec<_>>();

        let selected = if self.control {
            toggle_drag_selection(&self.baseline.selected, &hit_paths, layouts)
        } else {
            hit_paths.clone()
        };
        let primary = hit_paths.last().cloned();
        let anchor = primary.clone();

        DragSelectionResult {
            selected,
            primary,
            anchor,
            rect: Some(rect),
            active: true,
        }
    }
}

pub(super) fn drag_distance(start: DragPoint, end: DragPoint) -> f32 {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    (delta_x * delta_x + delta_y * delta_y).sqrt()
}

pub(super) fn toggle_drag_selection(
    baseline: &[PathBuf],
    hits: &[PathBuf],
    layouts: &[VisibleItemLayout],
) -> Vec<PathBuf> {
    let mut toggled = baseline.to_vec();
    for hit in hits {
        if let Some(index) = toggled.iter().position(|path| path == hit) {
            toggled.remove(index);
        } else {
            toggled.push(hit.clone());
        }
    }

    let mut ordered = layouts
        .iter()
        .filter_map(|layout| {
            toggled
                .iter()
                .any(|path| path == &layout.path)
                .then(|| layout.path.clone())
        })
        .collect::<Vec<_>>();

    for path in toggled {
        if ordered.iter().any(|existing| existing == &path) {
            continue;
        }
        ordered.push(path);
    }

    ordered
}
