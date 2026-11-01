//! `DensityVolume`: прямоугольная решётка блоков с шагом; индекс — y, затем x, затем z.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume {
    pub size: [i32; 3],
    pub min: [i32; 3],
    pub step: [i32; 3],
}

impl Volume {
    pub fn new(size: [i32; 3], min: [i32; 3], step: [i32; 3]) -> Self {
        debug_assert!(size.iter().all(|&s| s > 0) && step.iter().all(|&s| s > 0));
        Self { size, min, step }
    }

    pub fn blocks(size: [i32; 3], min: [i32; 3]) -> Self {
        Self::new(size, min, [1, 1, 1])
    }

    pub fn len(&self) -> usize {
        (self.size[0] * self.size[1] * self.size[2]) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn index(&self, x: i32, y: i32, z: i32) -> usize {
        (y + (x + z * self.size[0]) * self.size[1]) as usize
    }

    pub fn block_x(&self, i: i32) -> i32 {
        self.min[0] + i * self.step[0]
    }

    pub fn block_y(&self, i: i32) -> i32 {
        self.min[1] + i * self.step[1]
    }

    pub fn block_z(&self, i: i32) -> i32 {
        self.min[2] + i * self.step[2]
    }

    pub fn max_block(&self, axis: usize) -> i32 {
        self.min[axis] + self.size[axis] * self.step[axis] - 1
    }

    /// `indexOfBlock`: индекс блока или `None`, если его нет в решётке.
    pub fn index_of(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let r = [x - self.min[0], y - self.min[1], z - self.min[2]];
        for (a, &v) in r.iter().enumerate() {
            if v < 0 || v >= self.size[a] * self.step[a] || v.rem_euclid(self.step[a]) != 0 {
                return None;
            }
        }
        Some(self.index(
            r[0].div_euclid(self.step[0]),
            r[1].div_euclid(self.step[1]),
            r[2].div_euclid(self.step[2]),
        ))
    }
}
