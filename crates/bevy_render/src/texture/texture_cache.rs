use crate::{
    render_resource::{Texture, TextureView},
    renderer::RenderDevice,
};
use bevy_ecs::{prelude::ResMut, resource::Resource};
use bevy_platform::collections::{hash_map::Entry, HashMap};
use wgpu::{
    TextureAspect, TextureDescriptor, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

#[derive(Clone, PartialEq, Eq, Hash)]
struct TextureViewKey {
    format: Option<TextureFormat>,
    dimension: Option<TextureViewDimension>,
    aspect: TextureAspect,
    base_mip_level: u32,
    mip_level_count: Option<u32>,
    base_array_layer: u32,
    array_layer_count: Option<u32>,
}

impl From<&TextureViewDescriptor<'static>> for TextureViewKey {
    fn from(descriptor: &TextureViewDescriptor<'static>) -> Self {
        Self {
            format: descriptor.format,
            dimension: descriptor.dimension,
            aspect: descriptor.aspect,
            base_mip_level: descriptor.base_mip_level,
            mip_level_count: descriptor.mip_level_count,
            base_array_layer: descriptor.base_array_layer,
            array_layer_count: descriptor.array_layer_count,
        }
    }
}

/// The internal representation of a [`CachedTexture`] used to track whether it was recently used
/// and is currently taken.
struct CachedTextureMeta {
    texture: Texture,
    default_view: TextureView,
    taken: bool,
    frames_since_last_use: usize,
    views: HashMap<TextureViewKey, TextureView>,
}

/// A cached GPU [`Texture`] with corresponding [`TextureView`].
///
/// This is useful for textures that are created repeatedly (each frame) in the rendering process
/// to reduce the amount of GPU memory allocations.
#[derive(Clone)]
pub struct CachedTexture {
    pub texture: Texture,
    pub default_view: TextureView,
}

/// This resource caches textures that are created repeatedly in the rendering process and
/// are only required for one frame.
#[derive(Resource, Default)]
pub struct TextureCache {
    textures: HashMap<TextureDescriptor<'static>, Vec<CachedTextureMeta>>,
}

impl TextureCache {
    /// Retrieves a texture that matches the `descriptor`. If no matching one is found a new
    /// [`CachedTexture`] is created.
    pub fn get(
        &mut self,
        render_device: &RenderDevice,
        descriptor: TextureDescriptor<'static>,
    ) -> CachedTexture {
        match self.textures.entry(descriptor) {
            Entry::Occupied(mut entry) => {
                for texture in entry.get_mut().iter_mut() {
                    if !texture.taken {
                        texture.frames_since_last_use = 0;
                        texture.taken = true;
                        return CachedTexture {
                            texture: texture.texture.clone(),
                            default_view: texture.default_view.clone(),
                        };
                    }
                }

                let texture = render_device.create_texture(&entry.key().clone());
                let default_view = texture.create_view(&TextureViewDescriptor::default());
                entry.get_mut().push(CachedTextureMeta {
                    texture: texture.clone(),
                    default_view: default_view.clone(),
                    frames_since_last_use: 0,
                    taken: true,
                    views: HashMap::default(),
                });
                CachedTexture {
                    texture,
                    default_view,
                }
            }
            Entry::Vacant(entry) => {
                let texture = render_device.create_texture(entry.key());
                let default_view = texture.create_view(&TextureViewDescriptor::default());
                entry.insert(vec![CachedTextureMeta {
                    texture: texture.clone(),
                    default_view: default_view.clone(),
                    taken: true,
                    frames_since_last_use: 0,
                    views: HashMap::default(),
                }]);
                CachedTexture {
                    texture,
                    default_view,
                }
            }
        }
    }

    /// Retrieves a view for the given `texture` that matches the `view_descriptor`.
    /// If no matching one is found a new [`TextureView`] is created.
    ///
    /// The `texture_descriptor` must match the one used to create the texture.
    pub fn get_view(
        &mut self,
        texture: &Texture,
        texture_descriptor: &TextureDescriptor<'static>,
        view_descriptor: &TextureViewDescriptor<'static>,
    ) -> TextureView {
        if let Some(textures) = self.textures.get_mut(texture_descriptor) {
            for meta in textures {
                if meta.texture.id() == texture.id() {
                    match meta.views.entry(view_descriptor.into()) {
                        Entry::Occupied(entry) => return entry.get().clone(),
                        Entry::Vacant(entry) => {
                            let view = texture.create_view(view_descriptor);
                            entry.insert(view.clone());
                            return view;
                        }
                    }
                }
            }
        }

        // If the texture was not found in the cache, create a new view without caching it.
        texture.create_view(view_descriptor)
    }

    /// Returns `true` if the texture cache contains no textures.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Updates the cache and only retains recently used textures.
    pub fn update(&mut self) {
        self.textures.retain(|_, textures| {
            for texture in textures.iter_mut() {
                texture.frames_since_last_use += 1;
                texture.taken = false;
            }

            textures.retain(|texture| texture.frames_since_last_use < 3);
            !textures.is_empty()
        });
    }
}

/// Updates the [`TextureCache`] to only retains recently used textures.
pub fn update_texture_cache_system(mut texture_cache: ResMut<TextureCache>) {
    texture_cache.update();
}
