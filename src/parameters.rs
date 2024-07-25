use std::{fmt::Display, marker::PhantomData, str::FromStr};

use ark_crypto_primitives::merkle_tree::{Config, LeafParam, TwoToOneParam};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum SoundnessType {
    UniqueDecoding,
    ProvableList,
    ConjectureList,
}

impl Display for SoundnessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                SoundnessType::ProvableList => "ProvableList",
                SoundnessType::ConjectureList => "ConjectureList",
                SoundnessType::UniqueDecoding => "UniqueDecoding",
            }
        )
    }
}

impl FromStr for SoundnessType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "ProvableList" {
            Ok(SoundnessType::ProvableList)
        } else if s == "ConjectureList" {
            Ok(SoundnessType::ConjectureList)
        } else if s == "UniqueDecoding" {
            Ok(SoundnessType::UniqueDecoding)
        } else {
            Err(format!("Invalid soundness specification: {}", s))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MultivariateParameters<F> {
    pub(crate) num_variables: usize,
    _field: PhantomData<F>,
}

impl<F> MultivariateParameters<F> {
    pub fn new(num_variables: usize) -> Self {
        Self {
            num_variables,
            _field: PhantomData,
        }
    }
}

impl<F> Display for MultivariateParameters<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Number of variables: {}", self.num_variables)
    }
}

#[derive(Clone)]
pub struct WhirParameters<MerkleConfig>
where
    MerkleConfig: Config,
{
    pub starting_log_inv_rate: usize,
    pub folding_factor: usize,
    pub soundness_type: SoundnessType,
    pub security_level: usize,
    pub protocol_security_level: usize,

    // Merkle tree parameters
    pub leaf_hash_params: LeafParam<MerkleConfig>,
    pub two_to_one_params: TwoToOneParam<MerkleConfig>,
}

impl<MerkleConfig> Display for WhirParameters<MerkleConfig>
where
    MerkleConfig: Config,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Targeting {}-bits of security - protocol running at {}-bits - soundness: {:?}",
            self.security_level, self.protocol_security_level, self.soundness_type
        )?;
        writeln!(
            f,
            "Starting rate: 2^-{}, folding_factor: {}",
            self.starting_log_inv_rate, self.folding_factor
        )
    }
}