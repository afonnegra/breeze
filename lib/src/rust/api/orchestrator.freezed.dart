// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'orchestrator.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$UiStateDto {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiStateDto()';
}


}

/// @nodoc
class $UiStateDtoCopyWith<$Res>  {
$UiStateDtoCopyWith(UiStateDto _, $Res Function(UiStateDto) __);
}


/// Adds pattern-matching-related methods to [UiStateDto].
extension UiStateDtoPatterns on UiStateDto {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( UiStateDto_Listening value)?  listening,TResult Function( UiStateDto_Transcribing value)?  transcribing,TResult Function( UiStateDto_Injecting value)?  injecting,TResult Function( UiStateDto_Error value)?  error,TResult Function( UiStateDto_Hidden value)?  hidden,required TResult orElse(),}){
final _that = this;
switch (_that) {
case UiStateDto_Listening() when listening != null:
return listening(_that);case UiStateDto_Transcribing() when transcribing != null:
return transcribing(_that);case UiStateDto_Injecting() when injecting != null:
return injecting(_that);case UiStateDto_Error() when error != null:
return error(_that);case UiStateDto_Hidden() when hidden != null:
return hidden(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( UiStateDto_Listening value)  listening,required TResult Function( UiStateDto_Transcribing value)  transcribing,required TResult Function( UiStateDto_Injecting value)  injecting,required TResult Function( UiStateDto_Error value)  error,required TResult Function( UiStateDto_Hidden value)  hidden,}){
final _that = this;
switch (_that) {
case UiStateDto_Listening():
return listening(_that);case UiStateDto_Transcribing():
return transcribing(_that);case UiStateDto_Injecting():
return injecting(_that);case UiStateDto_Error():
return error(_that);case UiStateDto_Hidden():
return hidden(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( UiStateDto_Listening value)?  listening,TResult? Function( UiStateDto_Transcribing value)?  transcribing,TResult? Function( UiStateDto_Injecting value)?  injecting,TResult? Function( UiStateDto_Error value)?  error,TResult? Function( UiStateDto_Hidden value)?  hidden,}){
final _that = this;
switch (_that) {
case UiStateDto_Listening() when listening != null:
return listening(_that);case UiStateDto_Transcribing() when transcribing != null:
return transcribing(_that);case UiStateDto_Injecting() when injecting != null:
return injecting(_that);case UiStateDto_Error() when error != null:
return error(_that);case UiStateDto_Hidden() when hidden != null:
return hidden(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  listening,TResult Function()?  transcribing,TResult Function()?  injecting,TResult Function( String message)?  error,TResult Function()?  hidden,required TResult orElse(),}) {final _that = this;
switch (_that) {
case UiStateDto_Listening() when listening != null:
return listening();case UiStateDto_Transcribing() when transcribing != null:
return transcribing();case UiStateDto_Injecting() when injecting != null:
return injecting();case UiStateDto_Error() when error != null:
return error(_that.message);case UiStateDto_Hidden() when hidden != null:
return hidden();case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  listening,required TResult Function()  transcribing,required TResult Function()  injecting,required TResult Function( String message)  error,required TResult Function()  hidden,}) {final _that = this;
switch (_that) {
case UiStateDto_Listening():
return listening();case UiStateDto_Transcribing():
return transcribing();case UiStateDto_Injecting():
return injecting();case UiStateDto_Error():
return error(_that.message);case UiStateDto_Hidden():
return hidden();}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  listening,TResult? Function()?  transcribing,TResult? Function()?  injecting,TResult? Function( String message)?  error,TResult? Function()?  hidden,}) {final _that = this;
switch (_that) {
case UiStateDto_Listening() when listening != null:
return listening();case UiStateDto_Transcribing() when transcribing != null:
return transcribing();case UiStateDto_Injecting() when injecting != null:
return injecting();case UiStateDto_Error() when error != null:
return error(_that.message);case UiStateDto_Hidden() when hidden != null:
return hidden();case _:
  return null;

}
}

}

/// @nodoc


class UiStateDto_Listening extends UiStateDto {
  const UiStateDto_Listening(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto_Listening);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiStateDto.listening()';
}


}




/// @nodoc


class UiStateDto_Transcribing extends UiStateDto {
  const UiStateDto_Transcribing(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto_Transcribing);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiStateDto.transcribing()';
}


}




/// @nodoc


class UiStateDto_Injecting extends UiStateDto {
  const UiStateDto_Injecting(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto_Injecting);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiStateDto.injecting()';
}


}




/// @nodoc


class UiStateDto_Error extends UiStateDto {
  const UiStateDto_Error({required this.message}): super._();
  

 final  String message;

/// Create a copy of UiStateDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$UiStateDto_ErrorCopyWith<UiStateDto_Error> get copyWith => _$UiStateDto_ErrorCopyWithImpl<UiStateDto_Error>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto_Error&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,message);

@override
String toString() {
  return 'UiStateDto.error(message: $message)';
}


}

/// @nodoc
abstract mixin class $UiStateDto_ErrorCopyWith<$Res> implements $UiStateDtoCopyWith<$Res> {
  factory $UiStateDto_ErrorCopyWith(UiStateDto_Error value, $Res Function(UiStateDto_Error) _then) = _$UiStateDto_ErrorCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$UiStateDto_ErrorCopyWithImpl<$Res>
    implements $UiStateDto_ErrorCopyWith<$Res> {
  _$UiStateDto_ErrorCopyWithImpl(this._self, this._then);

  final UiStateDto_Error _self;
  final $Res Function(UiStateDto_Error) _then;

/// Create a copy of UiStateDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(UiStateDto_Error(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class UiStateDto_Hidden extends UiStateDto {
  const UiStateDto_Hidden(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is UiStateDto_Hidden);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'UiStateDto.hidden()';
}


}




// dart format on
