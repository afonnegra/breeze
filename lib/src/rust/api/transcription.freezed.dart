// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'transcription.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$TranscriptionError {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TranscriptionError()';
}


}

/// @nodoc
class $TranscriptionErrorCopyWith<$Res>  {
$TranscriptionErrorCopyWith(TranscriptionError _, $Res Function(TranscriptionError) __);
}


/// Adds pattern-matching-related methods to [TranscriptionError].
extension TranscriptionErrorPatterns on TranscriptionError {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( TranscriptionError_ModelMissing value)?  modelMissing,TResult Function( TranscriptionError_ModelCorrupt value)?  modelCorrupt,TResult Function( TranscriptionError_EngineLoadFailed value)?  engineLoadFailed,TResult Function( TranscriptionError_Busy value)?  busy,TResult Function( TranscriptionError_TranscribeFailed value)?  transcribeFailed,TResult Function( TranscriptionError_NotInitialized value)?  notInitialized,required TResult orElse(),}){
final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing() when modelMissing != null:
return modelMissing(_that);case TranscriptionError_ModelCorrupt() when modelCorrupt != null:
return modelCorrupt(_that);case TranscriptionError_EngineLoadFailed() when engineLoadFailed != null:
return engineLoadFailed(_that);case TranscriptionError_Busy() when busy != null:
return busy(_that);case TranscriptionError_TranscribeFailed() when transcribeFailed != null:
return transcribeFailed(_that);case TranscriptionError_NotInitialized() when notInitialized != null:
return notInitialized(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( TranscriptionError_ModelMissing value)  modelMissing,required TResult Function( TranscriptionError_ModelCorrupt value)  modelCorrupt,required TResult Function( TranscriptionError_EngineLoadFailed value)  engineLoadFailed,required TResult Function( TranscriptionError_Busy value)  busy,required TResult Function( TranscriptionError_TranscribeFailed value)  transcribeFailed,required TResult Function( TranscriptionError_NotInitialized value)  notInitialized,}){
final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing():
return modelMissing(_that);case TranscriptionError_ModelCorrupt():
return modelCorrupt(_that);case TranscriptionError_EngineLoadFailed():
return engineLoadFailed(_that);case TranscriptionError_Busy():
return busy(_that);case TranscriptionError_TranscribeFailed():
return transcribeFailed(_that);case TranscriptionError_NotInitialized():
return notInitialized(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( TranscriptionError_ModelMissing value)?  modelMissing,TResult? Function( TranscriptionError_ModelCorrupt value)?  modelCorrupt,TResult? Function( TranscriptionError_EngineLoadFailed value)?  engineLoadFailed,TResult? Function( TranscriptionError_Busy value)?  busy,TResult? Function( TranscriptionError_TranscribeFailed value)?  transcribeFailed,TResult? Function( TranscriptionError_NotInitialized value)?  notInitialized,}){
final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing() when modelMissing != null:
return modelMissing(_that);case TranscriptionError_ModelCorrupt() when modelCorrupt != null:
return modelCorrupt(_that);case TranscriptionError_EngineLoadFailed() when engineLoadFailed != null:
return engineLoadFailed(_that);case TranscriptionError_Busy() when busy != null:
return busy(_that);case TranscriptionError_TranscribeFailed() when transcribeFailed != null:
return transcribeFailed(_that);case TranscriptionError_NotInitialized() when notInitialized != null:
return notInitialized(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  modelMissing,TResult Function()?  modelCorrupt,TResult Function( String field0)?  engineLoadFailed,TResult Function()?  busy,TResult Function( String field0)?  transcribeFailed,TResult Function()?  notInitialized,required TResult orElse(),}) {final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing() when modelMissing != null:
return modelMissing();case TranscriptionError_ModelCorrupt() when modelCorrupt != null:
return modelCorrupt();case TranscriptionError_EngineLoadFailed() when engineLoadFailed != null:
return engineLoadFailed(_that.field0);case TranscriptionError_Busy() when busy != null:
return busy();case TranscriptionError_TranscribeFailed() when transcribeFailed != null:
return transcribeFailed(_that.field0);case TranscriptionError_NotInitialized() when notInitialized != null:
return notInitialized();case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  modelMissing,required TResult Function()  modelCorrupt,required TResult Function( String field0)  engineLoadFailed,required TResult Function()  busy,required TResult Function( String field0)  transcribeFailed,required TResult Function()  notInitialized,}) {final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing():
return modelMissing();case TranscriptionError_ModelCorrupt():
return modelCorrupt();case TranscriptionError_EngineLoadFailed():
return engineLoadFailed(_that.field0);case TranscriptionError_Busy():
return busy();case TranscriptionError_TranscribeFailed():
return transcribeFailed(_that.field0);case TranscriptionError_NotInitialized():
return notInitialized();}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  modelMissing,TResult? Function()?  modelCorrupt,TResult? Function( String field0)?  engineLoadFailed,TResult? Function()?  busy,TResult? Function( String field0)?  transcribeFailed,TResult? Function()?  notInitialized,}) {final _that = this;
switch (_that) {
case TranscriptionError_ModelMissing() when modelMissing != null:
return modelMissing();case TranscriptionError_ModelCorrupt() when modelCorrupt != null:
return modelCorrupt();case TranscriptionError_EngineLoadFailed() when engineLoadFailed != null:
return engineLoadFailed(_that.field0);case TranscriptionError_Busy() when busy != null:
return busy();case TranscriptionError_TranscribeFailed() when transcribeFailed != null:
return transcribeFailed(_that.field0);case TranscriptionError_NotInitialized() when notInitialized != null:
return notInitialized();case _:
  return null;

}
}

}

/// @nodoc


class TranscriptionError_ModelMissing extends TranscriptionError {
  const TranscriptionError_ModelMissing(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_ModelMissing);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TranscriptionError.modelMissing()';
}


}




/// @nodoc


class TranscriptionError_ModelCorrupt extends TranscriptionError {
  const TranscriptionError_ModelCorrupt(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_ModelCorrupt);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TranscriptionError.modelCorrupt()';
}


}




/// @nodoc


class TranscriptionError_EngineLoadFailed extends TranscriptionError {
  const TranscriptionError_EngineLoadFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of TranscriptionError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TranscriptionError_EngineLoadFailedCopyWith<TranscriptionError_EngineLoadFailed> get copyWith => _$TranscriptionError_EngineLoadFailedCopyWithImpl<TranscriptionError_EngineLoadFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_EngineLoadFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'TranscriptionError.engineLoadFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $TranscriptionError_EngineLoadFailedCopyWith<$Res> implements $TranscriptionErrorCopyWith<$Res> {
  factory $TranscriptionError_EngineLoadFailedCopyWith(TranscriptionError_EngineLoadFailed value, $Res Function(TranscriptionError_EngineLoadFailed) _then) = _$TranscriptionError_EngineLoadFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$TranscriptionError_EngineLoadFailedCopyWithImpl<$Res>
    implements $TranscriptionError_EngineLoadFailedCopyWith<$Res> {
  _$TranscriptionError_EngineLoadFailedCopyWithImpl(this._self, this._then);

  final TranscriptionError_EngineLoadFailed _self;
  final $Res Function(TranscriptionError_EngineLoadFailed) _then;

/// Create a copy of TranscriptionError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(TranscriptionError_EngineLoadFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class TranscriptionError_Busy extends TranscriptionError {
  const TranscriptionError_Busy(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_Busy);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TranscriptionError.busy()';
}


}




/// @nodoc


class TranscriptionError_TranscribeFailed extends TranscriptionError {
  const TranscriptionError_TranscribeFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of TranscriptionError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$TranscriptionError_TranscribeFailedCopyWith<TranscriptionError_TranscribeFailed> get copyWith => _$TranscriptionError_TranscribeFailedCopyWithImpl<TranscriptionError_TranscribeFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_TranscribeFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'TranscriptionError.transcribeFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $TranscriptionError_TranscribeFailedCopyWith<$Res> implements $TranscriptionErrorCopyWith<$Res> {
  factory $TranscriptionError_TranscribeFailedCopyWith(TranscriptionError_TranscribeFailed value, $Res Function(TranscriptionError_TranscribeFailed) _then) = _$TranscriptionError_TranscribeFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$TranscriptionError_TranscribeFailedCopyWithImpl<$Res>
    implements $TranscriptionError_TranscribeFailedCopyWith<$Res> {
  _$TranscriptionError_TranscribeFailedCopyWithImpl(this._self, this._then);

  final TranscriptionError_TranscribeFailed _self;
  final $Res Function(TranscriptionError_TranscribeFailed) _then;

/// Create a copy of TranscriptionError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(TranscriptionError_TranscribeFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class TranscriptionError_NotInitialized extends TranscriptionError {
  const TranscriptionError_NotInitialized(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is TranscriptionError_NotInitialized);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'TranscriptionError.notInitialized()';
}


}




// dart format on
