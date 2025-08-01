use bevy_app::App;
use bevy_ecs::prelude::*;
use modul_util::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use bevy_ecs::entity::EntityHasher;

#[derive(Resource)]
pub struct Assets<T> {
    next: usize,
    // FIXME maybe use RwLock per asset instead of map of assets...
    assets: HashMap<usize, T>,
}

pub struct AssetId<T: Send + Sync + 'static>(usize, PhantomData<T>);

impl<T: Send + Sync + 'static> Hash for AssetId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: Send + Sync + 'static> PartialEq for AssetId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T: Send + Sync + 'static> Eq for AssetId<T> {}

impl<T: Send + Sync + 'static> Clone for AssetId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for AssetId<T> {}

impl<T: Send + Sync + 'static> Assets<T> {
    pub fn new() -> Self {
        Self {
            next: 0,
            assets: HashMap::new(),
        }
    }

    /// Returns an empty [AssetId]
    pub fn add_empty(&mut self) -> AssetId<T> {
        self.next += 1;
        AssetId(self.next - 1, PhantomData)
    }

    /// Adds an asset and returns its id
    pub fn add(&mut self, asset: T) -> AssetId<T> {
        let id = self.add_empty();
        self.replace(id, asset);
        id
    }

    pub fn contains(&self, id: &AssetId<T>) -> bool {
        self.assets.contains_key(&id.0)
    }

    /// Immutably gets an asset from an id
    pub fn get(&self, asset_id: AssetId<T>) -> Option<&T> {
        self.assets.get(&asset_id.0)
    }

    /// Mutably gets an asset from an id
    pub fn get_mut(&mut self, asset_id: AssetId<T>) -> Option<&mut T> {
        self.assets.get_mut(&asset_id.0)
    }

    /// Puts a new value in an asset, all AssetIds pointing to the old asset will now point to the new asset
    pub fn replace(&mut self, asset_id: AssetId<T>, asset: T) -> Option<T> {
        self.assets.insert(asset_id.0, asset)
    }

    /// Removes an asset leaving None in its place, a new asset can be put in its place using replace
    pub fn remove(&mut self, asset_id: AssetId<T>) -> Option<T> {
        self.assets.remove(&asset_id.0)
    }
}

/// Useful for asset "metadata" a bit like [EntityHashMaps](bevy_ecs::entity::EntityHashMap)
pub type AssetMap<K, V> = HashMap<AssetId<K>, V>;

pub trait AssetWorldExt {
    /// Adds an empty asset
    fn add_empty_asset<T: Send + Sync + 'static>(&mut self) -> AssetId<T>;
    /// Adds an asset and returns its id
    fn add_asset<T: Send + Sync + 'static>(&mut self, asset: T) -> AssetId<T>;
    /// Checks if a given asset exists
    fn has_asset<T: Send + Sync + 'static>(&self, asset: AssetId<T>) -> bool;

    /// Gets an asset from an id
    fn get_asset<T: Send + Sync + 'static>(&self, asset_id: AssetId<T>) -> Option<&T>;

    /// Gets an asset from an id
    fn get_asset_mut<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Option<Mut<T>>;

    /// gets and unwraps the given asset id
    fn asset<T: Send + Sync + 'static>(&self, asset_id: AssetId<T>) -> &T;

    /// get and unwraps the given asset id mutably
    fn asset_mut<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Mut<T>;

    /// Gets an asset from an id and runs a function on it, if the asset is not found the function is not run
    fn with_asset<T: Send + Sync + 'static, F: FnOnce(&mut T)>(
        &mut self,
        asset_id: AssetId<T>,
        f: F,
    );
    /// Like [with_asset] but also gives access to the world, this is done by removing the asset and adding it back in the end
    fn asset_scope<T: Send + Sync + 'static, F: FnOnce(&mut Self, &mut T)>(
        &mut self,
        asset_id: AssetId<T>,
        f: F,
    );
    /// Replaces an asset using [Assets::replace]
    fn replace_asset<T: Send + Sync + 'static>(
        &mut self,
        asset_id: AssetId<T>,
        asset: T,
    ) -> Option<T>;
    /// Removes an asset using [Assets::remove]
    fn remove_asset<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Option<T>;
}

impl AssetWorldExt for World {
    #[inline]
    fn add_empty_asset<T: Send + Sync + 'static>(&mut self) -> AssetId<T> {
        self.resource_mut::<Assets<T>>().add_empty()
    }

    #[inline]
    fn add_asset<T: Send + Sync + 'static>(&mut self, asset: T) -> AssetId<T> {
        self.resource_mut::<Assets<T>>().add(asset)
    }

    #[inline]
    fn has_asset<T: Send + Sync + 'static>(&self, asset: AssetId<T>) -> bool {
        self.resource::<Assets<T>>().contains(&asset)
    }

    #[inline]
    fn get_asset<T: Send + Sync + 'static>(&self, asset_id: AssetId<T>) -> Option<&T> {
        self.get_resource::<Assets<T>>()?.get(asset_id)
    }

    #[inline]
    fn get_asset_mut<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Option<Mut<T>> {
        if self.has_asset(asset_id) {
            Some(
                self.resource_mut::<Assets<T>>()
                    .map_unchanged(|assets| assets.get_mut(asset_id).unwrap()),
            )
        } else {
            None
        }
    }

    #[inline]
    fn asset<T: Send + Sync + 'static>(&self, asset_id: AssetId<T>) -> &T {
        self.get_asset(asset_id).unwrap()
    }

    #[inline]
    fn asset_mut<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Mut<T> {
        self.get_asset_mut(asset_id).unwrap()
    }

    #[inline]
    fn with_asset<T: Send + Sync + 'static, F: FnOnce(&mut T)>(
        &mut self,
        asset_id: AssetId<T>,
        f: F,
    ) {
        self.get_resource_mut::<Assets<T>>()
            .map(|mut assets| assets.get_mut(asset_id).map(f));
    }

    #[inline]
    fn asset_scope<T: Send + Sync + 'static, F: FnOnce(&mut Self, &mut T)>(
        &mut self,
        asset_id: AssetId<T>,
        f: F,
    ) {
        let mut assset = match self.remove_asset(asset_id) {
            Some(a) => a,
            None => return,
        };
        f(self, &mut assset);
        self.resource_mut::<Assets<T>>().replace(asset_id, assset);
    }

    #[inline]
    fn replace_asset<T: Send + Sync + 'static>(
        &mut self,
        asset_id: AssetId<T>,
        asset: T,
    ) -> Option<T> {
        self.get_resource_mut::<Assets<T>>()?
            .replace(asset_id, asset)
    }

    #[inline]
    fn remove_asset<T: Send + Sync + 'static>(&mut self, asset_id: AssetId<T>) -> Option<T> {
        self.get_resource_mut::<Assets<T>>()?.remove(asset_id)
    }
}

pub trait AssetAppExt {
    fn init_assets<T: Send + Sync + 'static>(&mut self);
}

impl AssetAppExt for App {
    #[inline]
    fn init_assets<T: Send + Sync + 'static>(&mut self) {
        self.world_mut().insert_resource(Assets::<T>::new());
    }
}
