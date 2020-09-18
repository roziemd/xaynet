//! Masked objects.
//!
//! See the [mask module] documentation since this is a private module anyways.
//!
//! [mask module]: ../index.html

pub mod serialization;

use std::iter::Iterator;

use num::bigint::BigUint;
use thiserror::Error;

use crate::mask::config::MaskConfig;

#[derive(Error, Debug)]
#[error("the mask object is invalid: data is incompatible with the masking configuration")]
/// Errors related to invalid mask objects.
pub struct InvalidMaskObjectError;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
/// A mask object which represents either a mask or a masked model.
pub struct MaskMany {
    pub data: Vec<BigUint>,
    pub config: MaskConfig,
}

impl MaskMany {
    /// Creates a new mask object from the given masking configuration and the elements of the mask
    /// or masked model.
    pub fn new(config: MaskConfig, data: Vec<BigUint>) -> Self {
        Self { data, config }
    }

    /// Creates a new mask object from the given masking configuration and the elements of the mask
    /// or masked model.
    ///
    /// # Errors
    /// Fails if the elements of the mask object don't conform to the given masking configuration.
    pub fn new_checked(
        config: MaskConfig,
        data: Vec<BigUint>,
    ) -> Result<Self, InvalidMaskObjectError> {
        let obj = Self::new(config, data);
        if obj.is_valid() {
            Ok(obj)
        } else {
            Err(InvalidMaskObjectError)
        }
    }

    pub fn empty(config: MaskConfig, size: usize) -> Self {
        Self {
            data: Vec::with_capacity(size),
            config,
        }
    }

    /// Checks if the elements of this mask object conform to the given masking configuration.
    pub fn is_valid(&self) -> bool {
        let order = self.config.order();
        self.data.iter().all(|i| i < &order)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
/// A mask object which represents either a mask or a masked scalar.
pub struct MaskOne {
    pub data: BigUint,
    pub config: MaskConfig,
}

impl From<&MaskOne> for MaskMany {
    fn from(mask_one: &MaskOne) -> Self {
        Self::new(mask_one.config, vec![mask_one.data.clone()])
    }
}

impl From<MaskOne> for MaskMany {
    fn from(mask_one: MaskOne) -> Self {
        Self::new(mask_one.config, vec![mask_one.data])
    }
}

impl MaskOne {
    /// Creates a new mask object from the given masking configuration and the mask
    /// or masked scalar.
    pub fn new(config: MaskConfig, data: BigUint) -> Self {
        Self { data, config }
    }

    /// Creates a new mask object from the given masking configuration and the mask
    /// or masked scalar.
    ///
    /// # Errors
    /// Fails if the mask object doesn't conform to the given masking configuration.
    pub fn new_checked(config: MaskConfig, data: BigUint) -> Result<Self, InvalidMaskObjectError> {
        let obj = Self::new(config, data);
        if obj.is_valid() {
            Ok(obj)
        } else {
            Err(InvalidMaskObjectError)
        }
    }

    pub fn empty(config: MaskConfig) -> Self {
        Self {
            data: BigUint::from(1_usize), // NOTE not really empty
            config,
        }
    }

    /// Checks if this mask object conforms to the given masking configuration.
    pub fn is_valid(&self) -> bool {
        self.data < self.config.order()
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
/// A mask object wrapper around a `MaskMany`, `MaskOne` pair.
pub struct MaskObject {
    pub vector: MaskMany,
    pub scalar: MaskOne,
}

impl MaskObject {
    // TODO doc
    pub fn new(vector: MaskMany, scalar: MaskOne) -> Self {
        Self { vector, scalar }
    }

    // TODO perhaps no need
    pub fn new_unchecked(
        config_v: MaskConfig,
        data_v: Vec<BigUint>,
        config_s: MaskConfig,
        data_s: BigUint,
    ) -> Self {
        Self {
            vector: MaskMany::new(config_v, data_v),
            scalar: MaskOne::new(config_s, data_s),
        }
    }

    // TODO doc
    pub fn new_checked(
        config_v: MaskConfig,
        data_v: Vec<BigUint>,
        config_s: MaskConfig,
        data_s: BigUint,
    ) -> Result<Self, InvalidMaskObjectError> {
        let vector = MaskMany::new_checked(config_v, data_v)?;
        let scalar = MaskOne::new_checked(config_s, data_s)?;
        Ok(Self { vector, scalar })
    }

    pub fn empty(config_many: MaskConfig, config_one: MaskConfig, size: usize) -> Self {
        Self {
            vector: MaskMany::empty(config_many, size),
            scalar: MaskOne::empty(config_one),
        }
    }

    // TODO doc
    pub fn is_valid(&self) -> bool {
        self.vector.is_valid() && self.scalar.is_valid()
    }
}
