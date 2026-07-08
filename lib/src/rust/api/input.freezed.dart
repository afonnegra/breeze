// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'input.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$HotkeyEventDto {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HotkeyEventDto);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'HotkeyEventDto()';
}


}

/// @nodoc
class $HotkeyEventDtoCopyWith<$Res>  {
$HotkeyEventDtoCopyWith(HotkeyEventDto _, $Res Function(HotkeyEventDto) __);
}


/// Adds pattern-matching-related methods to [HotkeyEventDto].
extension HotkeyEventDtoPatterns on HotkeyEventDto {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( HotkeyEventDto_ComboPressed value)?  comboPressed,TResult Function( HotkeyEventDto_ComboReleased value)?  comboReleased,TResult Function( HotkeyEventDto_SessionLocked value)?  sessionLocked,TResult Function( HotkeyEventDto_SessionUnlocked value)?  sessionUnlocked,required TResult orElse(),}){
final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed() when comboPressed != null:
return comboPressed(_that);case HotkeyEventDto_ComboReleased() when comboReleased != null:
return comboReleased(_that);case HotkeyEventDto_SessionLocked() when sessionLocked != null:
return sessionLocked(_that);case HotkeyEventDto_SessionUnlocked() when sessionUnlocked != null:
return sessionUnlocked(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( HotkeyEventDto_ComboPressed value)  comboPressed,required TResult Function( HotkeyEventDto_ComboReleased value)  comboReleased,required TResult Function( HotkeyEventDto_SessionLocked value)  sessionLocked,required TResult Function( HotkeyEventDto_SessionUnlocked value)  sessionUnlocked,}){
final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed():
return comboPressed(_that);case HotkeyEventDto_ComboReleased():
return comboReleased(_that);case HotkeyEventDto_SessionLocked():
return sessionLocked(_that);case HotkeyEventDto_SessionUnlocked():
return sessionUnlocked(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( HotkeyEventDto_ComboPressed value)?  comboPressed,TResult? Function( HotkeyEventDto_ComboReleased value)?  comboReleased,TResult? Function( HotkeyEventDto_SessionLocked value)?  sessionLocked,TResult? Function( HotkeyEventDto_SessionUnlocked value)?  sessionUnlocked,}){
final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed() when comboPressed != null:
return comboPressed(_that);case HotkeyEventDto_ComboReleased() when comboReleased != null:
return comboReleased(_that);case HotkeyEventDto_SessionLocked() when sessionLocked != null:
return sessionLocked(_that);case HotkeyEventDto_SessionUnlocked() when sessionUnlocked != null:
return sessionUnlocked(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  comboPressed,TResult Function( BigInt holdMs,  ReleaseReasonDto reason)?  comboReleased,TResult Function()?  sessionLocked,TResult Function()?  sessionUnlocked,required TResult orElse(),}) {final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed() when comboPressed != null:
return comboPressed();case HotkeyEventDto_ComboReleased() when comboReleased != null:
return comboReleased(_that.holdMs,_that.reason);case HotkeyEventDto_SessionLocked() when sessionLocked != null:
return sessionLocked();case HotkeyEventDto_SessionUnlocked() when sessionUnlocked != null:
return sessionUnlocked();case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  comboPressed,required TResult Function( BigInt holdMs,  ReleaseReasonDto reason)  comboReleased,required TResult Function()  sessionLocked,required TResult Function()  sessionUnlocked,}) {final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed():
return comboPressed();case HotkeyEventDto_ComboReleased():
return comboReleased(_that.holdMs,_that.reason);case HotkeyEventDto_SessionLocked():
return sessionLocked();case HotkeyEventDto_SessionUnlocked():
return sessionUnlocked();}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  comboPressed,TResult? Function( BigInt holdMs,  ReleaseReasonDto reason)?  comboReleased,TResult? Function()?  sessionLocked,TResult? Function()?  sessionUnlocked,}) {final _that = this;
switch (_that) {
case HotkeyEventDto_ComboPressed() when comboPressed != null:
return comboPressed();case HotkeyEventDto_ComboReleased() when comboReleased != null:
return comboReleased(_that.holdMs,_that.reason);case HotkeyEventDto_SessionLocked() when sessionLocked != null:
return sessionLocked();case HotkeyEventDto_SessionUnlocked() when sessionUnlocked != null:
return sessionUnlocked();case _:
  return null;

}
}

}

/// @nodoc


class HotkeyEventDto_ComboPressed extends HotkeyEventDto {
  const HotkeyEventDto_ComboPressed(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HotkeyEventDto_ComboPressed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'HotkeyEventDto.comboPressed()';
}


}




/// @nodoc


class HotkeyEventDto_ComboReleased extends HotkeyEventDto {
  const HotkeyEventDto_ComboReleased({required this.holdMs, required this.reason}): super._();
  

/// How long the combo was held, in milliseconds.
 final  BigInt holdMs;
/// What ended the combo.
 final  ReleaseReasonDto reason;

/// Create a copy of HotkeyEventDto
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$HotkeyEventDto_ComboReleasedCopyWith<HotkeyEventDto_ComboReleased> get copyWith => _$HotkeyEventDto_ComboReleasedCopyWithImpl<HotkeyEventDto_ComboReleased>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HotkeyEventDto_ComboReleased&&(identical(other.holdMs, holdMs) || other.holdMs == holdMs)&&(identical(other.reason, reason) || other.reason == reason));
}


@override
int get hashCode => Object.hash(runtimeType,holdMs,reason);

@override
String toString() {
  return 'HotkeyEventDto.comboReleased(holdMs: $holdMs, reason: $reason)';
}


}

/// @nodoc
abstract mixin class $HotkeyEventDto_ComboReleasedCopyWith<$Res> implements $HotkeyEventDtoCopyWith<$Res> {
  factory $HotkeyEventDto_ComboReleasedCopyWith(HotkeyEventDto_ComboReleased value, $Res Function(HotkeyEventDto_ComboReleased) _then) = _$HotkeyEventDto_ComboReleasedCopyWithImpl;
@useResult
$Res call({
 BigInt holdMs, ReleaseReasonDto reason
});




}
/// @nodoc
class _$HotkeyEventDto_ComboReleasedCopyWithImpl<$Res>
    implements $HotkeyEventDto_ComboReleasedCopyWith<$Res> {
  _$HotkeyEventDto_ComboReleasedCopyWithImpl(this._self, this._then);

  final HotkeyEventDto_ComboReleased _self;
  final $Res Function(HotkeyEventDto_ComboReleased) _then;

/// Create a copy of HotkeyEventDto
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? holdMs = null,Object? reason = null,}) {
  return _then(HotkeyEventDto_ComboReleased(
holdMs: null == holdMs ? _self.holdMs : holdMs // ignore: cast_nullable_to_non_nullable
as BigInt,reason: null == reason ? _self.reason : reason // ignore: cast_nullable_to_non_nullable
as ReleaseReasonDto,
  ));
}


}

/// @nodoc


class HotkeyEventDto_SessionLocked extends HotkeyEventDto {
  const HotkeyEventDto_SessionLocked(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HotkeyEventDto_SessionLocked);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'HotkeyEventDto.sessionLocked()';
}


}




/// @nodoc


class HotkeyEventDto_SessionUnlocked extends HotkeyEventDto {
  const HotkeyEventDto_SessionUnlocked(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is HotkeyEventDto_SessionUnlocked);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'HotkeyEventDto.sessionUnlocked()';
}


}




/// @nodoc
mixin _$InputError {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError()';
}


}

/// @nodoc
class $InputErrorCopyWith<$Res>  {
$InputErrorCopyWith(InputError _, $Res Function(InputError) __);
}


/// Adds pattern-matching-related methods to [InputError].
extension InputErrorPatterns on InputError {
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

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( InputError_MonitorAlreadyRunning value)?  monitorAlreadyRunning,TResult Function( InputError_MonitorStartFailed value)?  monitorStartFailed,TResult Function( InputError_MonitorNotRunning value)?  monitorNotRunning,TResult Function( InputError_AudioNoDevice value)?  audioNoDevice,TResult Function( InputError_AudioInitFailed value)?  audioInitFailed,TResult Function( InputError_AudioNotPrewarmed value)?  audioNotPrewarmed,TResult Function( InputError_InjectionFailed value)?  injectionFailed,TResult Function( InputError_OrchestratorAlreadyRunning value)?  orchestratorAlreadyRunning,TResult Function( InputError_OrchestratorNotRunning value)?  orchestratorNotRunning,TResult Function( InputError_OrchestratorStartFailed value)?  orchestratorStartFailed,TResult Function( InputError_OverlayWindowNotFound value)?  overlayWindowNotFound,TResult Function( InputError_OverlayStyleFailed value)?  overlayStyleFailed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning() when monitorAlreadyRunning != null:
return monitorAlreadyRunning(_that);case InputError_MonitorStartFailed() when monitorStartFailed != null:
return monitorStartFailed(_that);case InputError_MonitorNotRunning() when monitorNotRunning != null:
return monitorNotRunning(_that);case InputError_AudioNoDevice() when audioNoDevice != null:
return audioNoDevice(_that);case InputError_AudioInitFailed() when audioInitFailed != null:
return audioInitFailed(_that);case InputError_AudioNotPrewarmed() when audioNotPrewarmed != null:
return audioNotPrewarmed(_that);case InputError_InjectionFailed() when injectionFailed != null:
return injectionFailed(_that);case InputError_OrchestratorAlreadyRunning() when orchestratorAlreadyRunning != null:
return orchestratorAlreadyRunning(_that);case InputError_OrchestratorNotRunning() when orchestratorNotRunning != null:
return orchestratorNotRunning(_that);case InputError_OrchestratorStartFailed() when orchestratorStartFailed != null:
return orchestratorStartFailed(_that);case InputError_OverlayWindowNotFound() when overlayWindowNotFound != null:
return overlayWindowNotFound(_that);case InputError_OverlayStyleFailed() when overlayStyleFailed != null:
return overlayStyleFailed(_that);case _:
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

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( InputError_MonitorAlreadyRunning value)  monitorAlreadyRunning,required TResult Function( InputError_MonitorStartFailed value)  monitorStartFailed,required TResult Function( InputError_MonitorNotRunning value)  monitorNotRunning,required TResult Function( InputError_AudioNoDevice value)  audioNoDevice,required TResult Function( InputError_AudioInitFailed value)  audioInitFailed,required TResult Function( InputError_AudioNotPrewarmed value)  audioNotPrewarmed,required TResult Function( InputError_InjectionFailed value)  injectionFailed,required TResult Function( InputError_OrchestratorAlreadyRunning value)  orchestratorAlreadyRunning,required TResult Function( InputError_OrchestratorNotRunning value)  orchestratorNotRunning,required TResult Function( InputError_OrchestratorStartFailed value)  orchestratorStartFailed,required TResult Function( InputError_OverlayWindowNotFound value)  overlayWindowNotFound,required TResult Function( InputError_OverlayStyleFailed value)  overlayStyleFailed,}){
final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning():
return monitorAlreadyRunning(_that);case InputError_MonitorStartFailed():
return monitorStartFailed(_that);case InputError_MonitorNotRunning():
return monitorNotRunning(_that);case InputError_AudioNoDevice():
return audioNoDevice(_that);case InputError_AudioInitFailed():
return audioInitFailed(_that);case InputError_AudioNotPrewarmed():
return audioNotPrewarmed(_that);case InputError_InjectionFailed():
return injectionFailed(_that);case InputError_OrchestratorAlreadyRunning():
return orchestratorAlreadyRunning(_that);case InputError_OrchestratorNotRunning():
return orchestratorNotRunning(_that);case InputError_OrchestratorStartFailed():
return orchestratorStartFailed(_that);case InputError_OverlayWindowNotFound():
return overlayWindowNotFound(_that);case InputError_OverlayStyleFailed():
return overlayStyleFailed(_that);}
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

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( InputError_MonitorAlreadyRunning value)?  monitorAlreadyRunning,TResult? Function( InputError_MonitorStartFailed value)?  monitorStartFailed,TResult? Function( InputError_MonitorNotRunning value)?  monitorNotRunning,TResult? Function( InputError_AudioNoDevice value)?  audioNoDevice,TResult? Function( InputError_AudioInitFailed value)?  audioInitFailed,TResult? Function( InputError_AudioNotPrewarmed value)?  audioNotPrewarmed,TResult? Function( InputError_InjectionFailed value)?  injectionFailed,TResult? Function( InputError_OrchestratorAlreadyRunning value)?  orchestratorAlreadyRunning,TResult? Function( InputError_OrchestratorNotRunning value)?  orchestratorNotRunning,TResult? Function( InputError_OrchestratorStartFailed value)?  orchestratorStartFailed,TResult? Function( InputError_OverlayWindowNotFound value)?  overlayWindowNotFound,TResult? Function( InputError_OverlayStyleFailed value)?  overlayStyleFailed,}){
final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning() when monitorAlreadyRunning != null:
return monitorAlreadyRunning(_that);case InputError_MonitorStartFailed() when monitorStartFailed != null:
return monitorStartFailed(_that);case InputError_MonitorNotRunning() when monitorNotRunning != null:
return monitorNotRunning(_that);case InputError_AudioNoDevice() when audioNoDevice != null:
return audioNoDevice(_that);case InputError_AudioInitFailed() when audioInitFailed != null:
return audioInitFailed(_that);case InputError_AudioNotPrewarmed() when audioNotPrewarmed != null:
return audioNotPrewarmed(_that);case InputError_InjectionFailed() when injectionFailed != null:
return injectionFailed(_that);case InputError_OrchestratorAlreadyRunning() when orchestratorAlreadyRunning != null:
return orchestratorAlreadyRunning(_that);case InputError_OrchestratorNotRunning() when orchestratorNotRunning != null:
return orchestratorNotRunning(_that);case InputError_OrchestratorStartFailed() when orchestratorStartFailed != null:
return orchestratorStartFailed(_that);case InputError_OverlayWindowNotFound() when overlayWindowNotFound != null:
return overlayWindowNotFound(_that);case InputError_OverlayStyleFailed() when overlayStyleFailed != null:
return overlayStyleFailed(_that);case _:
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

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function()?  monitorAlreadyRunning,TResult Function( String field0)?  monitorStartFailed,TResult Function()?  monitorNotRunning,TResult Function()?  audioNoDevice,TResult Function( String field0)?  audioInitFailed,TResult Function()?  audioNotPrewarmed,TResult Function( String field0)?  injectionFailed,TResult Function()?  orchestratorAlreadyRunning,TResult Function()?  orchestratorNotRunning,TResult Function( String field0)?  orchestratorStartFailed,TResult Function( String field0)?  overlayWindowNotFound,TResult Function( String field0)?  overlayStyleFailed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning() when monitorAlreadyRunning != null:
return monitorAlreadyRunning();case InputError_MonitorStartFailed() when monitorStartFailed != null:
return monitorStartFailed(_that.field0);case InputError_MonitorNotRunning() when monitorNotRunning != null:
return monitorNotRunning();case InputError_AudioNoDevice() when audioNoDevice != null:
return audioNoDevice();case InputError_AudioInitFailed() when audioInitFailed != null:
return audioInitFailed(_that.field0);case InputError_AudioNotPrewarmed() when audioNotPrewarmed != null:
return audioNotPrewarmed();case InputError_InjectionFailed() when injectionFailed != null:
return injectionFailed(_that.field0);case InputError_OrchestratorAlreadyRunning() when orchestratorAlreadyRunning != null:
return orchestratorAlreadyRunning();case InputError_OrchestratorNotRunning() when orchestratorNotRunning != null:
return orchestratorNotRunning();case InputError_OrchestratorStartFailed() when orchestratorStartFailed != null:
return orchestratorStartFailed(_that.field0);case InputError_OverlayWindowNotFound() when overlayWindowNotFound != null:
return overlayWindowNotFound(_that.field0);case InputError_OverlayStyleFailed() when overlayStyleFailed != null:
return overlayStyleFailed(_that.field0);case _:
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

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function()  monitorAlreadyRunning,required TResult Function( String field0)  monitorStartFailed,required TResult Function()  monitorNotRunning,required TResult Function()  audioNoDevice,required TResult Function( String field0)  audioInitFailed,required TResult Function()  audioNotPrewarmed,required TResult Function( String field0)  injectionFailed,required TResult Function()  orchestratorAlreadyRunning,required TResult Function()  orchestratorNotRunning,required TResult Function( String field0)  orchestratorStartFailed,required TResult Function( String field0)  overlayWindowNotFound,required TResult Function( String field0)  overlayStyleFailed,}) {final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning():
return monitorAlreadyRunning();case InputError_MonitorStartFailed():
return monitorStartFailed(_that.field0);case InputError_MonitorNotRunning():
return monitorNotRunning();case InputError_AudioNoDevice():
return audioNoDevice();case InputError_AudioInitFailed():
return audioInitFailed(_that.field0);case InputError_AudioNotPrewarmed():
return audioNotPrewarmed();case InputError_InjectionFailed():
return injectionFailed(_that.field0);case InputError_OrchestratorAlreadyRunning():
return orchestratorAlreadyRunning();case InputError_OrchestratorNotRunning():
return orchestratorNotRunning();case InputError_OrchestratorStartFailed():
return orchestratorStartFailed(_that.field0);case InputError_OverlayWindowNotFound():
return overlayWindowNotFound(_that.field0);case InputError_OverlayStyleFailed():
return overlayStyleFailed(_that.field0);}
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

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function()?  monitorAlreadyRunning,TResult? Function( String field0)?  monitorStartFailed,TResult? Function()?  monitorNotRunning,TResult? Function()?  audioNoDevice,TResult? Function( String field0)?  audioInitFailed,TResult? Function()?  audioNotPrewarmed,TResult? Function( String field0)?  injectionFailed,TResult? Function()?  orchestratorAlreadyRunning,TResult? Function()?  orchestratorNotRunning,TResult? Function( String field0)?  orchestratorStartFailed,TResult? Function( String field0)?  overlayWindowNotFound,TResult? Function( String field0)?  overlayStyleFailed,}) {final _that = this;
switch (_that) {
case InputError_MonitorAlreadyRunning() when monitorAlreadyRunning != null:
return monitorAlreadyRunning();case InputError_MonitorStartFailed() when monitorStartFailed != null:
return monitorStartFailed(_that.field0);case InputError_MonitorNotRunning() when monitorNotRunning != null:
return monitorNotRunning();case InputError_AudioNoDevice() when audioNoDevice != null:
return audioNoDevice();case InputError_AudioInitFailed() when audioInitFailed != null:
return audioInitFailed(_that.field0);case InputError_AudioNotPrewarmed() when audioNotPrewarmed != null:
return audioNotPrewarmed();case InputError_InjectionFailed() when injectionFailed != null:
return injectionFailed(_that.field0);case InputError_OrchestratorAlreadyRunning() when orchestratorAlreadyRunning != null:
return orchestratorAlreadyRunning();case InputError_OrchestratorNotRunning() when orchestratorNotRunning != null:
return orchestratorNotRunning();case InputError_OrchestratorStartFailed() when orchestratorStartFailed != null:
return orchestratorStartFailed(_that.field0);case InputError_OverlayWindowNotFound() when overlayWindowNotFound != null:
return overlayWindowNotFound(_that.field0);case InputError_OverlayStyleFailed() when overlayStyleFailed != null:
return overlayStyleFailed(_that.field0);case _:
  return null;

}
}

}

/// @nodoc


class InputError_MonitorAlreadyRunning extends InputError {
  const InputError_MonitorAlreadyRunning(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_MonitorAlreadyRunning);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.monitorAlreadyRunning()';
}


}




/// @nodoc


class InputError_MonitorStartFailed extends InputError {
  const InputError_MonitorStartFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_MonitorStartFailedCopyWith<InputError_MonitorStartFailed> get copyWith => _$InputError_MonitorStartFailedCopyWithImpl<InputError_MonitorStartFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_MonitorStartFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.monitorStartFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_MonitorStartFailedCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_MonitorStartFailedCopyWith(InputError_MonitorStartFailed value, $Res Function(InputError_MonitorStartFailed) _then) = _$InputError_MonitorStartFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_MonitorStartFailedCopyWithImpl<$Res>
    implements $InputError_MonitorStartFailedCopyWith<$Res> {
  _$InputError_MonitorStartFailedCopyWithImpl(this._self, this._then);

  final InputError_MonitorStartFailed _self;
  final $Res Function(InputError_MonitorStartFailed) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_MonitorStartFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class InputError_MonitorNotRunning extends InputError {
  const InputError_MonitorNotRunning(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_MonitorNotRunning);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.monitorNotRunning()';
}


}




/// @nodoc


class InputError_AudioNoDevice extends InputError {
  const InputError_AudioNoDevice(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_AudioNoDevice);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.audioNoDevice()';
}


}




/// @nodoc


class InputError_AudioInitFailed extends InputError {
  const InputError_AudioInitFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_AudioInitFailedCopyWith<InputError_AudioInitFailed> get copyWith => _$InputError_AudioInitFailedCopyWithImpl<InputError_AudioInitFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_AudioInitFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.audioInitFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_AudioInitFailedCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_AudioInitFailedCopyWith(InputError_AudioInitFailed value, $Res Function(InputError_AudioInitFailed) _then) = _$InputError_AudioInitFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_AudioInitFailedCopyWithImpl<$Res>
    implements $InputError_AudioInitFailedCopyWith<$Res> {
  _$InputError_AudioInitFailedCopyWithImpl(this._self, this._then);

  final InputError_AudioInitFailed _self;
  final $Res Function(InputError_AudioInitFailed) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_AudioInitFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class InputError_AudioNotPrewarmed extends InputError {
  const InputError_AudioNotPrewarmed(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_AudioNotPrewarmed);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.audioNotPrewarmed()';
}


}




/// @nodoc


class InputError_InjectionFailed extends InputError {
  const InputError_InjectionFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_InjectionFailedCopyWith<InputError_InjectionFailed> get copyWith => _$InputError_InjectionFailedCopyWithImpl<InputError_InjectionFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_InjectionFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.injectionFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_InjectionFailedCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_InjectionFailedCopyWith(InputError_InjectionFailed value, $Res Function(InputError_InjectionFailed) _then) = _$InputError_InjectionFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_InjectionFailedCopyWithImpl<$Res>
    implements $InputError_InjectionFailedCopyWith<$Res> {
  _$InputError_InjectionFailedCopyWithImpl(this._self, this._then);

  final InputError_InjectionFailed _self;
  final $Res Function(InputError_InjectionFailed) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_InjectionFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class InputError_OrchestratorAlreadyRunning extends InputError {
  const InputError_OrchestratorAlreadyRunning(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_OrchestratorAlreadyRunning);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.orchestratorAlreadyRunning()';
}


}




/// @nodoc


class InputError_OrchestratorNotRunning extends InputError {
  const InputError_OrchestratorNotRunning(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_OrchestratorNotRunning);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'InputError.orchestratorNotRunning()';
}


}




/// @nodoc


class InputError_OrchestratorStartFailed extends InputError {
  const InputError_OrchestratorStartFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_OrchestratorStartFailedCopyWith<InputError_OrchestratorStartFailed> get copyWith => _$InputError_OrchestratorStartFailedCopyWithImpl<InputError_OrchestratorStartFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_OrchestratorStartFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.orchestratorStartFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_OrchestratorStartFailedCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_OrchestratorStartFailedCopyWith(InputError_OrchestratorStartFailed value, $Res Function(InputError_OrchestratorStartFailed) _then) = _$InputError_OrchestratorStartFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_OrchestratorStartFailedCopyWithImpl<$Res>
    implements $InputError_OrchestratorStartFailedCopyWith<$Res> {
  _$InputError_OrchestratorStartFailedCopyWithImpl(this._self, this._then);

  final InputError_OrchestratorStartFailed _self;
  final $Res Function(InputError_OrchestratorStartFailed) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_OrchestratorStartFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class InputError_OverlayWindowNotFound extends InputError {
  const InputError_OverlayWindowNotFound(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_OverlayWindowNotFoundCopyWith<InputError_OverlayWindowNotFound> get copyWith => _$InputError_OverlayWindowNotFoundCopyWithImpl<InputError_OverlayWindowNotFound>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_OverlayWindowNotFound&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.overlayWindowNotFound(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_OverlayWindowNotFoundCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_OverlayWindowNotFoundCopyWith(InputError_OverlayWindowNotFound value, $Res Function(InputError_OverlayWindowNotFound) _then) = _$InputError_OverlayWindowNotFoundCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_OverlayWindowNotFoundCopyWithImpl<$Res>
    implements $InputError_OverlayWindowNotFoundCopyWith<$Res> {
  _$InputError_OverlayWindowNotFoundCopyWithImpl(this._self, this._then);

  final InputError_OverlayWindowNotFound _self;
  final $Res Function(InputError_OverlayWindowNotFound) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_OverlayWindowNotFound(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class InputError_OverlayStyleFailed extends InputError {
  const InputError_OverlayStyleFailed(this.field0): super._();
  

 final  String field0;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$InputError_OverlayStyleFailedCopyWith<InputError_OverlayStyleFailed> get copyWith => _$InputError_OverlayStyleFailedCopyWithImpl<InputError_OverlayStyleFailed>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is InputError_OverlayStyleFailed&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode => Object.hash(runtimeType,field0);

@override
String toString() {
  return 'InputError.overlayStyleFailed(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $InputError_OverlayStyleFailedCopyWith<$Res> implements $InputErrorCopyWith<$Res> {
  factory $InputError_OverlayStyleFailedCopyWith(InputError_OverlayStyleFailed value, $Res Function(InputError_OverlayStyleFailed) _then) = _$InputError_OverlayStyleFailedCopyWithImpl;
@useResult
$Res call({
 String field0
});




}
/// @nodoc
class _$InputError_OverlayStyleFailedCopyWithImpl<$Res>
    implements $InputError_OverlayStyleFailedCopyWith<$Res> {
  _$InputError_OverlayStyleFailedCopyWithImpl(this._self, this._then);

  final InputError_OverlayStyleFailed _self;
  final $Res Function(InputError_OverlayStyleFailed) _then;

/// Create a copy of InputError
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(InputError_OverlayStyleFailed(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
