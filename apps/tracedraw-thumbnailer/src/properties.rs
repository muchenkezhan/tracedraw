//! The drawing's saved rating, exposed to the Windows property system.

use serde::de::{MapAccess, Visitor};
use serde::Deserialize;
use std::io::{self, Read};
use std::marker::PhantomData;

/// Bound the work done for one file, including streams without a size.
const MAX_PROPERTY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Default, Deserialize)]
struct DrawingProperties {
    #[serde(default)]
    metadata: Object<RatingMetadata>,
}

#[derive(Default, Deserialize)]
struct RatingMetadata {
    #[serde(default)]
    rating: u8,
}

/// Named JSON fields must be objects. Serde's default struct visitor also
/// accepts arrays, which are not metadata records in a drawing.
#[derive(Default)]
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a drawing metadata object")
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Object)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Read only the metadata. Serde skips the other fields without building
/// shapes, decoding embedded images or initializing the renderer and fonts.
fn read_rating(reader: impl Read) -> serde_json::Result<u32> {
    read_rating_with_limit(reader, MAX_PROPERTY_BYTES)
}

fn read_rating_with_limit(reader: impl Read, limit: u64) -> serde_json::Result<u32> {
    let reader = LimitedReader {
        inner: reader,
        remaining: limit,
    };
    let properties: Object<DrawingProperties> =
        serde_json::from_reader(io::BufReader::new(reader))?;
    Ok(shell_rating(properties.0.metadata.0.rating))
}

fn shell_rating(rating: u8) -> u32 {
    match rating.min(5) {
        0 => 0,
        1 => 1,
        2 => 25,
        3 => 50,
        4 => 75,
        _ => 99,
    }
}

/// Check one byte beyond the budget, so truncation cannot turn an
/// overlarge stream into an apparently complete, valid JSON document.
struct LimitedReader<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let available = buffer.len().min(self.remaining.saturating_add(1) as usize);
        let (buffer, _) = buffer.split_at_mut(available);
        let count = self.inner.read(buffer)?;
        if count as u64 > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "drawing is too large",
            ));
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

#[cfg(windows)]
pub(super) mod com {
    use super::read_rating;
    use std::io::{self, Read};
    use std::sync::Mutex;
    use windows::core::{implement, Error, Ref, Result, GUID, HRESULT};
    use windows::Win32::Foundation::{
        ERROR_ALREADY_INITIALIZED, E_FAIL, E_INVALIDARG, E_POINTER, E_UNEXPECTED, PROPERTYKEY,
        STG_E_ACCESSDENIED, S_FALSE,
    };
    use windows::Win32::System::Com::IStream;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IInitializeWithStream, IInitializeWithStream_Impl, IPropertyStore,
        IPropertyStoreCapabilities, IPropertyStoreCapabilities_Impl, IPropertyStore_Impl,
    };

    pub(crate) const CLSID: GUID = GUID::from_u128(0xA8E8D07A_71BB_4F15_BD73_2FA694E57C35);

    // System.Rating's published PROPERTYKEY, shared by every Windows format.
    const RATING_KEY: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x64440492_4c8b_11d1_8b70_080036b11a03),
        pid: 9,
    };

    /// The cached scalar is the only state shared between property calls.
    /// Initialization consumes its stream on the calling thread, so the
    /// handler can subsequently be queried safely from either apartment.
    #[implement(IInitializeWithStream, IPropertyStore, IPropertyStoreCapabilities)]
    pub(crate) struct Provider {
        rating: Mutex<Option<std::result::Result<u32, HRESULT>>>,
    }

    impl Provider {
        pub(crate) fn new() -> Self {
            crate::com::LIVE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Self {
                rating: Mutex::new(None),
            }
        }

        fn value(&self) -> Result<u32> {
            let rating = self
                .rating
                .lock()
                .map_err(|_| Error::from_hresult(E_FAIL))?;
            match *rating {
                Some(Ok(value)) => Ok(value),
                Some(Err(error)) => Err(error.into()),
                None => Err(E_UNEXPECTED.into()),
            }
        }
    }

    impl Drop for Provider {
        fn drop(&mut self) {
            crate::com::LIVE.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl IInitializeWithStream_Impl for Provider_Impl {
        fn Initialize(&self, stream: Ref<'_, IStream>, _mode: u32) -> Result<()> {
            let stream = stream.ok()?;
            let mut rating = self
                .rating
                .lock()
                .map_err(|_| Error::from_hresult(E_FAIL))?;
            if rating.is_some() {
                return Err(HRESULT::from_win32(ERROR_ALREADY_INITIALIZED.0).into());
            }
            let result = read_rating(StreamReader(stream)).map_err(|_| E_FAIL);
            *rating = Some(result);
            result.map(|_| ()).map_err(Error::from_hresult)
        }
    }

    impl IPropertyStore_Impl for Provider_Impl {
        fn GetCount(&self) -> Result<u32> {
            self.value()?;
            Ok(1)
        }

        fn GetAt(&self, index: u32, key: *mut PROPERTYKEY) -> Result<()> {
            if key.is_null() {
                return Err(E_POINTER.into());
            }
            // SAFETY: COM supplies a writable output key. Initialize it even
            // when the caller asks for an unavailable property.
            unsafe { key.write(PROPERTYKEY::default()) };
            self.value()?;
            if index != 0 {
                return Err(E_INVALIDARG.into());
            }
            unsafe { key.write(RATING_KEY) };
            Ok(())
        }

        fn GetValue(&self, key: *const PROPERTYKEY) -> Result<PROPVARIANT> {
            if key.is_null() {
                return Err(E_POINTER.into());
            }
            let rating = self.value()?;
            // SAFETY: COM supplies a readable property key.
            if unsafe { *key } == RATING_KEY {
                Ok(PROPVARIANT::from(rating))
            } else {
                Ok(PROPVARIANT::default())
            }
        }

        fn SetValue(&self, key: *const PROPERTYKEY, value: *const PROPVARIANT) -> Result<()> {
            if key.is_null() || value.is_null() {
                return Err(E_POINTER.into());
            }
            Err(STG_E_ACCESSDENIED.into())
        }

        fn Commit(&self) -> Result<()> {
            Err(STG_E_ACCESSDENIED.into())
        }
    }

    impl IPropertyStoreCapabilities_Impl for Provider_Impl {
        fn IsPropertyWritable(&self, key: *const PROPERTYKEY) -> HRESULT {
            if key.is_null() {
                E_POINTER
            } else {
                S_FALSE
            }
        }
    }

    struct StreamReader<'a>(&'a IStream);

    impl Read for StreamReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let requested = buffer.len().min(u32::MAX as usize) as u32;
            let mut count = 0u32;
            // SAFETY: the writable slice holds at least `requested` bytes and
            // `count` lives until the synchronous read returns.
            let status = unsafe {
                self.0
                    .Read(buffer.as_mut_ptr().cast(), requested, Some(&mut count))
            };
            if status.is_err() || count > requested {
                return Err(io::Error::other("drawing stream read failed"));
            }
            Ok(count as usize)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::ffi::c_void;
        use windows::core::{IUnknown, Interface};
        use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, S_OK};
        use windows::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, IClassFactory, COINIT_MULTITHREADED,
        };
        use windows::Win32::UI::Shell::PropertiesSystem::PSGetPropertyKeyFromName;
        use windows::Win32::UI::Shell::{IThumbnailProvider, SHCreateMemStream};

        // All COM tests share the unload counter and run under this lock.
        static COM_TESTS: Mutex<()> = Mutex::new(());

        struct Apartment;

        impl Apartment {
            fn enter() -> Self {
                // Both S_OK and S_FALSE require one matching uninitialize.
                unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                    .ok()
                    .expect("COM apartment");
                Self
            }
        }

        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }

        fn class_factory(clsid: &GUID) -> IClassFactory {
            let mut factory: Option<IClassFactory> = None;
            let status = unsafe {
                crate::com::DllGetClassObject(
                    clsid,
                    &IClassFactory::IID,
                    (&mut factory as *mut Option<IClassFactory>).cast(),
                )
            };
            status.ok().expect("class factory");
            factory.expect("factory output")
        }

        fn initialized(bytes: &[u8]) -> IPropertyStore {
            let factory = class_factory(&CLSID);
            let store: IPropertyStore =
                unsafe { factory.CreateInstance(None) }.expect("property handler");
            let initialize: IInitializeWithStream = store.cast().expect("stream initializer");
            let stream = unsafe { SHCreateMemStream(Some(bytes)) }.expect("memory stream");
            unsafe { initialize.Initialize(&stream, 0) }.expect("initialize metadata");
            store
        }

        #[test]
        fn property_handler_exposes_native_ratings_through_com() {
            let _serial = COM_TESTS.lock().expect("test lock");
            let _apartment = Apartment::enter();
            for (stars, value) in [(0, 0), (1, 1), (2, 25), (3, 50), (4, 75), (5, 99)] {
                let json = format!(r#"{{"metadata":{{"rating":{stars}}}}}"#);
                let store = initialized(json.as_bytes());
                assert_eq!(unsafe { store.GetCount() }.expect("count"), 1);
                let mut key = PROPERTYKEY::default();
                unsafe { store.GetAt(0, &mut key) }.expect("rating key");
                assert_eq!(key, RATING_KEY);
                let mut system_key = PROPERTYKEY::default();
                unsafe {
                    PSGetPropertyKeyFromName(windows::core::w!("System.Rating"), &mut system_key)
                }
                .expect("system rating schema");
                assert_eq!(key, system_key);
                let rating = unsafe { store.GetValue(&key) }.expect("rating value");
                assert_eq!(u32::try_from(&rating).expect("DWORD"), value);
                assert_eq!(
                    unsafe { store.GetAt(1, &mut key) }
                        .expect_err("invalid index")
                        .code(),
                    E_INVALIDARG
                );
                assert_eq!(key, PROPERTYKEY::default());
                assert!(unsafe { store.GetValue(&key) }
                    .expect("unknown key")
                    .is_empty());
            }
        }

        #[test]
        fn property_handler_is_read_only_and_rejects_invalid_keys() {
            let _serial = COM_TESTS.lock().expect("test lock");
            let _apartment = Apartment::enter();
            let store = initialized(br#"{"metadata":{"rating":4}}"#);
            let capabilities: IPropertyStoreCapabilities =
                store.cast().expect("read-only capability");
            assert_eq!(
                unsafe { capabilities.IsPropertyWritable(&RATING_KEY) },
                S_FALSE
            );
            assert_eq!(
                unsafe { capabilities.IsPropertyWritable(std::ptr::null()) },
                E_POINTER
            );
            assert_eq!(
                unsafe { store.GetAt(0, std::ptr::null_mut()) }
                    .expect_err("null key")
                    .code(),
                E_POINTER
            );
            assert_eq!(
                unsafe { store.GetValue(std::ptr::null()) }
                    .expect_err("null key")
                    .code(),
                E_POINTER
            );
            let replacement = PROPVARIANT::from(99u32);
            assert_eq!(
                unsafe { store.SetValue(&RATING_KEY, &replacement) }
                    .expect_err("read only")
                    .code(),
                STG_E_ACCESSDENIED
            );
            assert_eq!(
                unsafe { store.SetValue(std::ptr::null(), &replacement) }
                    .expect_err("null key")
                    .code(),
                E_POINTER
            );
            assert_eq!(
                unsafe { store.Commit() }.expect_err("read only").code(),
                STG_E_ACCESSDENIED
            );
            assert_eq!(
                u32::try_from(&unsafe { store.GetValue(&RATING_KEY) }.expect("unchanged"))
                    .expect("DWORD"),
                75
            );
        }

        #[test]
        fn property_handler_initializes_once_and_reports_bad_streams() {
            let _serial = COM_TESTS.lock().expect("test lock");
            let _apartment = Apartment::enter();
            let factory = class_factory(&CLSID);
            let store: IPropertyStore =
                unsafe { factory.CreateInstance(None) }.expect("property handler");
            let initialize: IInitializeWithStream = store.cast().expect("initializer");
            assert_eq!(
                unsafe { store.GetCount() }
                    .expect_err("not initialized")
                    .code(),
                E_UNEXPECTED
            );
            assert_eq!(
                unsafe { initialize.Initialize(None, 0) }
                    .expect_err("null stream")
                    .code(),
                E_POINTER
            );
            let stream = unsafe { SHCreateMemStream(Some(b"not json")) }.expect("memory stream");
            assert_eq!(
                unsafe { initialize.Initialize(&stream, 0) }
                    .expect_err("malformed drawing")
                    .code(),
                E_FAIL
            );
            assert_eq!(
                unsafe { store.GetValue(&RATING_KEY) }
                    .expect_err("failed metadata")
                    .code(),
                E_FAIL
            );
            assert_eq!(
                unsafe { initialize.Initialize(&stream, 0) }
                    .expect_err("already initialized")
                    .code(),
                HRESULT::from_win32(ERROR_ALREADY_INITIALIZED.0)
            );
        }

        #[test]
        fn both_classes_and_their_factories_keep_the_dll_loaded() {
            let _serial = COM_TESTS.lock().expect("test lock");
            let _apartment = Apartment::enter();
            assert_eq!(crate::com::DllCanUnloadNow(), S_OK);
            for clsid in [crate::com::CLSID, CLSID] {
                let factory = class_factory(&clsid);
                assert_eq!(crate::com::DllCanUnloadNow(), S_FALSE, "factory is alive");
                let instance: IUnknown = unsafe { factory.CreateInstance(None) }.expect("provider");
                if clsid == CLSID {
                    assert!(instance.cast::<IPropertyStore>().is_ok());
                    assert!(instance.cast::<IThumbnailProvider>().is_err());
                } else {
                    assert!(instance.cast::<IThumbnailProvider>().is_ok());
                    assert!(instance.cast::<IPropertyStore>().is_err());
                }
                let outer: IUnknown = instance.clone();
                assert_eq!(
                    unsafe { factory.CreateInstance::<_, IUnknown>(&outer) }
                        .expect_err("no aggregation")
                        .code(),
                    CLASS_E_NOAGGREGATION
                );
                drop(outer);
                drop(factory);
                assert_eq!(crate::com::DllCanUnloadNow(), S_FALSE, "provider is alive");
                drop(instance);
                assert_eq!(crate::com::DllCanUnloadNow(), S_OK);
            }
            let factory = class_factory(&CLSID);
            unsafe { factory.LockServer(true) }.expect("lock server");
            drop(factory);
            assert_eq!(
                crate::com::DllCanUnloadNow(),
                S_FALSE,
                "server lock is alive"
            );
            let factory = class_factory(&CLSID);
            unsafe { factory.LockServer(false) }.expect("unlock server");
            drop(factory);
            assert_eq!(crate::com::DllCanUnloadNow(), S_OK);

            let mut unknown: *mut c_void = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    crate::com::DllGetClassObject(
                        &GUID::zeroed(),
                        &IClassFactory::IID,
                        &mut unknown,
                    )
                },
                CLASS_E_CLASSNOTAVAILABLE
            );
            assert!(unknown.is_null());
            assert_eq!(
                unsafe {
                    crate::com::DllGetClassObject(
                        std::ptr::null(),
                        &IClassFactory::IID,
                        &mut unknown,
                    )
                },
                E_POINTER
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_ratings_match_the_saved_native_metadata() {
        for stars in 0..=5 {
            let mut drawing = tracedraw_core::Document::default();
            drawing.metadata.rating = stars;
            let json = drawing.to_json().expect("native drawing");
            assert_eq!(
                read_rating(json.as_bytes()).expect("metadata"),
                shell_rating(stars)
            );
        }
        assert_eq!(shell_rating(u8::MAX), 99);
    }

    #[test]
    fn older_drawings_without_a_rating_are_unrated() {
        for json in ["{}", r#"{"metadata":{}}"#, r#"{"title":"old","pages":[]}"#] {
            assert_eq!(read_rating(json.as_bytes()).expect("older metadata"), 0);
        }
    }

    #[test]
    fn unrelated_deep_and_large_fields_are_skipped() {
        let depth = tracedraw_core::document::MAX_JSON_DEPTH - 2;
        let json = format!(
            r#"{{"pages":{}0{},"bitmap":"{}","metadata":{{"rating":4}}}}"#,
            "[".repeat(depth),
            "]".repeat(depth),
            "a".repeat(128 * 1024)
        );
        assert_eq!(read_rating(json.as_bytes()).expect("skip content"), 75);
    }

    #[test]
    fn malformed_metadata_and_trailing_input_are_rejected() {
        for json in [
            "",
            "[]",
            "null",
            "not json",
            r#"{"metadata":null}"#,
            r#"{"metadata":[]}"#,
            r#"{"metadata":{"rating":"4"}}"#,
            r#"{"metadata":{"rating":-1}}"#,
            r#"{"metadata":{"rating":256}}"#,
            r#"{"metadata":{"rating":4,"rating":2}}"#,
            r#"{"metadata":{},"metadata":{}}"#,
            r#"{"metadata":{"rating":4}}x"#,
        ] {
            assert!(read_rating(json.as_bytes()).is_err(), "{json}");
        }
        assert!(read_rating(&[0xff, 0xfe][..]).is_err());
    }

    #[test]
    fn stream_size_limit_checks_the_complete_input() {
        let json = br#"{"metadata":{"rating":5}}"#;
        assert_eq!(
            read_rating_with_limit(&json[..], json.len() as u64).expect("exact limit"),
            99
        );
        assert!(read_rating_with_limit(&json[..], json.len() as u64 - 1).is_err());
        assert!(read_rating_with_limit(&b"{} "[..], 2).is_err());
    }
}
