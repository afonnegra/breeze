// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'config.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ConfigApiError {

 String get field0;
/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ConfigApiErrorCopyWith<ConfigApiError> get copyWith => _$ConfigApiErrorCopyWithImpl<ConfigApiError>(this as ConfigApiError, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConfigApiError&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'ConfigApiError(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $ConfigApiErrorCopyWith<$Res>  {
  factory $ConfigApiErrorCopyWith(ConfigApiError value, $Res Function(ConfigApiError) _then) = _$ConfigApiErrorCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$ConfigApiErrorCopyWithImpl<$Res>
    implements $ConfigApiErrorCopyWith<$Res> {
  _$ConfigApiErrorCopyWithImpl(this._self, this._then);

  final ConfigApiError _self;
  final $Res Function(ConfigApiError) _then;

/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? field0 = null,}) {
  return _then(_self.copyWith(
field0: null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}

}


/// Adds pattern-matching-related methods to [ConfigApiError].
extension ConfigApiErrorPatterns on ConfigApiError {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( ConfigApiError_UnsupportedLanguage value)?  unsupportedLanguage,TResult Function( ConfigApiError_Persist value)?  persist,required TResult orElse(),}){
final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage() when unsupportedLanguage != null:
return unsupportedLanguage(_that);case ConfigApiError_Persist() when persist != null:
return persist(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( ConfigApiError_UnsupportedLanguage value)  unsupportedLanguage,required TResult Function( ConfigApiError_Persist value)  persist,}){
final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage():
return unsupportedLanguage(_that);case ConfigApiError_Persist():
return persist(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( ConfigApiError_UnsupportedLanguage value)?  unsupportedLanguage,TResult? Function( ConfigApiError_Persist value)?  persist,}){
final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage() when unsupportedLanguage != null:
return unsupportedLanguage(_that);case ConfigApiError_Persist() when persist != null:
return persist(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( String field0)?  unsupportedLanguage,TResult Function( String field0)?  persist,required TResult orElse(),}) {final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage() when unsupportedLanguage != null:
return unsupportedLanguage(_that.field0);case ConfigApiError_Persist() when persist != null:
return persist(_that.field0);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( String field0)  unsupportedLanguage,required TResult Function( String field0)  persist,}) {final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage():
return unsupportedLanguage(_that.field0);case ConfigApiError_Persist():
return persist(_that.field0);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( String field0)?  unsupportedLanguage,TResult? Function( String field0)?  persist,}) {final _that = this;
switch (_that) {
case ConfigApiError_UnsupportedLanguage() when unsupportedLanguage != null:
return unsupportedLanguage(_that.field0);case ConfigApiError_Persist() when persist != null:
return persist(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class ConfigApiError_UnsupportedLanguage extends ConfigApiError {
  const ConfigApiError_UnsupportedLanguage(this.field0): super._();
  

@override final  String field0;

/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ConfigApiError_UnsupportedLanguageCopyWith<ConfigApiError_UnsupportedLanguage> get copyWith => _$ConfigApiError_UnsupportedLanguageCopyWithImpl<ConfigApiError_UnsupportedLanguage>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConfigApiError_UnsupportedLanguage&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'ConfigApiError.unsupportedLanguage(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $ConfigApiError_UnsupportedLanguageCopyWith<$Res> implements $ConfigApiErrorCopyWith<$Res> {
  factory $ConfigApiError_UnsupportedLanguageCopyWith(ConfigApiError_UnsupportedLanguage value, $Res Function(ConfigApiError_UnsupportedLanguage) _then) = _$ConfigApiError_UnsupportedLanguageCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$ConfigApiError_UnsupportedLanguageCopyWithImpl<$Res>
    implements $ConfigApiError_UnsupportedLanguageCopyWith<$Res> {
  _$ConfigApiError_UnsupportedLanguageCopyWithImpl(this._self, this._then);

  final ConfigApiError_UnsupportedLanguage _self;
  final $Res Function(ConfigApiError_UnsupportedLanguage) _then;

/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(ConfigApiError_UnsupportedLanguage(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class ConfigApiError_Persist extends ConfigApiError {
  const ConfigApiError_Persist(this.field0): super._();
  

@override final  String field0;

/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ConfigApiError_PersistCopyWith<ConfigApiError_Persist> get copyWith => _$ConfigApiError_PersistCopyWithImpl<ConfigApiError_Persist>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ConfigApiError_Persist&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'ConfigApiError.persist(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $ConfigApiError_PersistCopyWith<$Res> implements $ConfigApiErrorCopyWith<$Res> {
  factory $ConfigApiError_PersistCopyWith(ConfigApiError_Persist value, $Res Function(ConfigApiError_Persist) _then) = _$ConfigApiError_PersistCopyWithImpl;
@override @useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$ConfigApiError_PersistCopyWithImpl<$Res>
    implements $ConfigApiError_PersistCopyWith<$Res> {
  _$ConfigApiError_PersistCopyWithImpl(this._self, this._then);

  final ConfigApiError_Persist _self;
  final $Res Function(ConfigApiError_Persist) _then;

/// Create a copy of ConfigApiError
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(ConfigApiError_Persist(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
