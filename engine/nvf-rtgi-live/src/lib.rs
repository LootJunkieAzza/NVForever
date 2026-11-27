//! A cell's bounce light, to be solved on the GPU with ray queries as nvf-rtgi solves it on the CPU (#3009): the
//! same points, directions, surfaces, lamps and sums. This is NVForever's own solver, home-grown for this engine;
//! nvf-rtgi stays the reference that its results are held against.
//!
//! So far only the inputs, packed on the CPU for that future solve: [`Tables`] (the scene), [`PackedLight`] (a state
//! of the light) and [`Points`] (where the light is gathered) as the words a kernel reads, made on any thread. What a
//! shader could not compute as the solver does is refused here, with the reason, and stays the CPU solver's.

#![forbid(unsafe_code)]

mod tables;

pub use tables::{PackedLight, Points, Tables};
