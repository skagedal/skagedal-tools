import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/settings.dart';
import 'package:jikido/src/sitting_controller.dart';
import 'package:jikido/src/ui/sitting_page.dart';
import 'package:jikido/src/ui/theme.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'fakes.dart';

void main() {
  late FakeBellAudio audio;
  late FakeClosingBellNotification notification;
  late FakeSittingService service;
  late FakeScreenAwake screen;
  late TestClock clock;
  late SittingController controller;

  setUp(() {
    SharedPreferences.setMockInitialValues(<String, Object>{});
    audio = FakeBellAudio();
    notification = FakeClosingBellNotification();
    service = FakeSittingService();
    screen = FakeScreenAwake();
    clock = TestClock(DateTime.utc(2026, 3, 1, 7, 0, 0));
    controller = SittingController(
      audio: audio,
      notification: notification,
      service: service,
      screen: screen,
      clock: clock.call,
    );
  });

  tearDown(() => controller.dispose());

  /// Most of these tests are about the sitting rather than the settling time
  /// that now precedes it by default, so they turn it off.
  void withoutSettling() => controller.setPrepare(Duration.zero);

  Future<void> pumpPage(WidgetTester tester) => tester.pumpWidget(
        MaterialApp(
          theme: jikidoTheme(),
          home: SittingPage(controller: controller),
        ),
      );

  testWidgets('offers the presets and the default length', (tester) async {
    await pumpPage(tester);

    expect(find.text('JIKIDO'), findsOneWidget);
    expect(find.text('15'), findsNWidgets(2), reason: 'the face and its chip');
    expect(find.text('minutes'), findsOneWidget);
    expect(find.text('Sit'), findsOneWidget);
    for (final preset in Settings.presets) {
      expect(find.text('${preset.inMinutes}'), findsWidgets);
    }
  });

  testWidgets('choosing a preset changes the length shown', (tester) async {
    await pumpPage(tester);

    await tester.tap(find.widgetWithText(InkWell, '5'));
    await tester.pumpAndSettle();

    expect(controller.settings.duration, const Duration(minutes: 5));
    expect(find.text('minutes'), findsOneWidget);
  });

  testWidgets('sitting shows a countdown and a way out', (tester) async {
    withoutSettling();
    await pumpPage(tester);

    await tester.tap(find.text('Sit'));
    await tester.pump();

    expect(controller.status, SittingStatus.running);
    expect(find.text('15:00'), findsOneWidget);
    expect(find.text('opening bell'), findsOneWidget);
    expect(find.text('End sitting'), findsOneWidget);
    expect(find.text('Sit'), findsNothing);

    clock.advance(const Duration(minutes: 2, seconds: 15));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('12:45'), findsOneWidget);
    expect(find.text('remaining'), findsOneWidget);

    await tester.tap(find.text('End sitting'));
    await tester.pump();
    await tester.pump();
    expect(controller.status, SittingStatus.idle);
  });

  testWidgets('a failure to set up audio is shown, not swallowed',
      (tester) async {
    audio.failToInitialize = true;
    await controller.initialize();
    await pumpPage(tester);

    expect(controller.initializationError, isNotNull);
    expect(find.textContaining('could not set up'), findsOneWidget);
  });

  testWidgets('the settling time counts down before the sitting does',
      (tester) async {
    controller.setPrepare(const Duration(minutes: 1));
    await pumpPage(tester);

    await tester.tap(find.text('Sit'));
    await tester.pump();

    expect(controller.status, SittingStatus.running);
    expect(find.text('1:00'), findsOneWidget);
    expect(find.text('settling'), findsOneWidget);
    expect(find.text('15:00'), findsNothing,
        reason: 'the period has not opened, so its clock has not started');
    expect(audio.strikes, isEmpty);

    clock.advance(const Duration(seconds: 40));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('0:20'), findsOneWidget);

    clock.advance(const Duration(seconds: 20));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('15:00'), findsOneWidget,
        reason: 'the bell has rung and the sitting proper has begun');
    expect(find.text('opening bell'), findsOneWidget);

    // Leave nothing ticking behind us.
    await tester.tap(find.text('End sitting'));
    await tester.pump();
    await tester.pump();
  });

  testWidgets('a sitting can be held and taken up again', (tester) async {
    withoutSettling();
    await pumpPage(tester);

    await tester.tap(find.text('Sit'));
    await tester.pump();

    clock.advance(const Duration(minutes: 2));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('13:00'), findsOneWidget);

    await tester.tap(find.text('Pause'));
    await tester.pump();
    await tester.pump();

    expect(controller.isPaused, isTrue);
    expect(find.text('paused'), findsOneWidget);
    expect(find.text('Resume'), findsOneWidget);
    expect(find.text('Pause'), findsNothing);
    expect(find.text('End sitting'), findsOneWidget,
        reason: 'the way out is still there while held');

    clock.advance(const Duration(minutes: 10));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('13:00'), findsOneWidget,
        reason: 'ten minutes of pause is not ten minutes of sitting');

    await tester.tap(find.text('Resume'));
    await tester.pump();
    await tester.pump();

    expect(controller.isPaused, isFalse);
    expect(find.text('13:00'), findsOneWidget);
    expect(find.text('remaining'), findsOneWidget);

    await tester.tap(find.text('End sitting'));
    await tester.pump();
    await tester.pump();
    expect(controller.status, SittingStatus.idle);
  });

  testWidgets('the settling time can be held too', (tester) async {
    controller.setPrepare(const Duration(minutes: 1));
    await pumpPage(tester);

    await tester.tap(find.text('Sit'));
    await tester.pump();

    clock.advance(const Duration(seconds: 20));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('0:40'), findsOneWidget);

    await tester.tap(find.text('Pause'));
    await tester.pump();
    await tester.pump();

    expect(find.text('paused'), findsOneWidget);
    expect(find.text('settling'), findsNothing);

    clock.advance(const Duration(minutes: 5));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.text('0:40'), findsOneWidget);
    expect(audio.strikes, isEmpty, reason: 'the bell waited');

    await tester.tap(find.text('Resume'));
    await tester.pump();
    await tester.pump();
    expect(find.text('settling'), findsOneWidget);

    await tester.tap(find.text('End sitting'));
    await tester.pump();
    await tester.pump();
  });

  testWidgets('the bell can be rung on its own', (tester) async {
    withoutSettling();
    await pumpPage(tester);

    await tester.tap(find.byTooltip('Ring the bell'));
    await tester.pumpAndSettle();

    expect(find.text('strike'), findsOneWidget);
    expect(find.text('rest the striker'), findsOneWidget);

    await tester.tap(find.text('strike'));
    await tester.tap(find.text('strike'));
    await tester.pump();
    expect(audio.taps, 2, reason: 'strikes overlap rather than restarting');

    await tester.tap(find.text('rest the striker'));
    await tester.pump();
    expect(audio.damps, 1);
  });
}
