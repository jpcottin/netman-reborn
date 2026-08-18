//! Modèle : décodage des trames en métadonnées (`packet`), agrégation en
//! deux tables de conversations L2/L3 (`tables`), et vue Appman par
//! application (`apps`, consommée par le port Android).

pub mod apps;
pub mod packet;
pub mod tables;
