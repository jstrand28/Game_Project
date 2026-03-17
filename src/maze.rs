use bevy::prelude::*;
use rand::{rngs::StdRng, RngExt, SeedableRng};

#[derive(Clone, Copy, Debug)]
pub struct Cell { pub walls: [bool; 4] } // N,E,S,W

#[derive(Clone, Debug)]
pub struct Maze {
    pub w: u32, pub h: u32,
    pub cells: Vec<Cell>,
}
impl Maze {
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, cells: vec![Cell { walls: [true;4] }; (w*h) as usize] }
    }
    fn idx(&self, x: u32, y: u32) -> usize { (y*self.w + x) as usize }

    pub fn generate_with_three_exits(w: u32, h: u32, seed: u64) -> Self {
        let mut m = Self::new(w,h);
        m.carve(seed);
        m.open_three_exits(seed);
        m
    }

    fn carve(&mut self, seed: u64) {
        #[derive(Clone, Copy)] struct P(u32,u32);
        let mut rng = StdRng::seed_from_u64(seed);
        let (w,h) = (self.w, self.h);
        let (sx, sy) = (rng.random_range(0..w), rng.random_range(0..h));
        let mut visited = vec![false; (w*h) as usize];
        let mut stack = vec![P(sx,sy)];
        visited[self.idx(sx,sy)] = true;

        let dirs: &[(i32,i32,usize,usize)] = &[
            (0,-1,0,2),(1,0,1,3),(0,1,2,0),(-1,0,3,1)
        ];

        while let Some(&P(cx,cy)) = stack.last() {
            let mut nbrs = vec![];
            for &(dx,dy, wcur, wnbr) in dirs {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if nx>=0 && nx<w as i32 && ny>=0 && ny<h as i32 {
                    let (ux,uy) = (nx as u32, ny as u32);
                    if !visited[self.idx(ux,uy)] {
                        nbrs.push((ux,uy,wcur,wnbr));
                    }
                }
            }
            if nbrs.is_empty() { stack.pop(); }
            else {
                let (nx,ny,wc,wn) = nbrs[rng.random_range(0..nbrs.len())];
                let c = self.idx(cx,cy);
                let n = self.idx(nx,ny);
                self.cells[c].walls[wc] = false;
                self.cells[n].walls[wn] = false;
                visited[n] = true;
                stack.push(P(nx,ny));
            }
        }
    }

    fn open_three_exits(&mut self, seed: u64) {
        // choose three different border cells and open outward
        let mut rng = StdRng::seed_from_u64(seed ^ 0xDEADBEEF);
        let mut picks: Vec<(u32,u32,usize)> = vec![]; // x,y,wall idx to open
        // north, east, south
        let candidates = [
            (rng.random_range(0..self.w), 0, 0usize),                    // north -> open N
            (self.w-1, rng.random_range(0..self.h), 1usize),             // east  -> open E
            (rng.random_range(0..self.w), self.h-1, 2usize),             // south -> open S
        ];
        picks.extend_from_slice(&candidates);
        for (x,y,wi) in picks {
            let i = self.idx(x,y);
            self.cells[i].walls[wi] = false;
        }
    }
}

/// Marker for exit tiles in world
#[derive(Component)]
pub struct ExitMarker;