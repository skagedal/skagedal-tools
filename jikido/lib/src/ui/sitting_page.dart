import 'package:flutter/material.dart';

import '../session.dart';
import '../settings.dart';
import '../sitting_controller.dart';
import 'duration_sheet.dart';
import 'enso.dart';
import 'settings_page.dart';
import 'theme.dart';

/// The one screen that matters: choose a length, sit, hear the bell.
class SittingPage extends StatefulWidget {
  const SittingPage({super.key, required this.controller});

  final SittingController controller;

  @override
  State<SittingPage> createState() => _SittingPageState();
}

class _SittingPageState extends State<SittingPage> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      widget.controller.onResumed();
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.controller,
        builder: (context, _) => _build(context),
      );

  Widget _build(BuildContext context) {
    final controller = widget.controller;
    final running = controller.status == SittingStatus.running;

    return Scaffold(
      appBar: AppBar(
        title: const Text(
          'JIKIDO',
          style: TextStyle(letterSpacing: 6, fontSize: 14),
        ),
        actions: [
          // Settings are hidden mid-sitting; there is nothing in there worth
          // getting up for.
          if (!running)
            IconButton(
              icon: const Icon(Icons.more_horiz),
              tooltip: 'Settings',
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => SettingsPage(controller: controller),
                ),
              ),
            ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            if (controller.initializationError != null)
              _ErrorBanner(message: controller.initializationError!),
            Expanded(
              child: Center(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 40),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 340),
                    child: Enso(
                      progress: running ? controller.progress : 0,
                      child: _Face(controller: controller),
                    ),
                  ),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(24, 0, 24, 32),
              child: _Controls(controller: controller),
            ),
          ],
        ),
      ),
    );
  }
}

/// What is written inside the ensō.
class _Face extends StatelessWidget {
  const _Face({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) {
    switch (controller.status) {
      case SittingStatus.idle:
        return _Stack(
          big: '${controller.settings.duration.inMinutes}',
          small: controller.settings.duration.inMinutes == 1
              ? 'minute'
              : 'minutes',
        );
      case SittingStatus.running:
        return _Stack(
          big: formatRemaining(controller.remaining),
          small: _phaseCaption(controller.phase),
        );
      case SittingStatus.complete:
        return const _Stack(big: '—', small: 'sitting complete');
    }
  }

  static String _phaseCaption(SessionPhase? phase) => switch (phase) {
        SessionPhase.opening => 'opening bell',
        SessionPhase.closing => 'closing bell',
        _ => 'remaining',
      };
}

class _Stack extends StatelessWidget {
  const _Stack({required this.big, required this.small});

  final String big;
  final String small;

  @override
  Widget build(BuildContext context) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            big,
            style: const TextStyle(
              fontSize: 62,
              fontWeight: FontWeight.w200,
              letterSpacing: 1,
              height: 1.1,
              // Without this the countdown jitters as the digits change.
              fontFeatures: [FontFeature.tabularFigures()],
            ),
          ),
          const SizedBox(height: 6),
          Text(
            small,
            style: const TextStyle(
              fontSize: 13,
              letterSpacing: 2,
              color: JikidoColors.faded,
            ),
          ),
        ],
      );
}

class _Controls extends StatelessWidget {
  const _Controls({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) {
    switch (controller.status) {
      case SittingStatus.idle:
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _PresetRow(controller: controller),
            const SizedBox(height: 28),
            _PrimaryButton(label: 'Sit', onPressed: controller.start),
          ],
        );

      case SittingStatus.running:
        return TextButton(
          onPressed: controller.cancel,
          child: const Text(
            'End sitting',
            style: TextStyle(color: JikidoColors.faded, letterSpacing: 1),
          ),
        );

      case SittingStatus.complete:
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (controller.notice != null) ...[
              Text(
                controller.notice!,
                textAlign: TextAlign.center,
                style: const TextStyle(color: JikidoColors.faded, fontSize: 13),
              ),
              const SizedBox(height: 20),
            ],
            _PrimaryButton(label: 'Done', onPressed: controller.acknowledge),
          ],
        );
    }
  }
}

class _PresetRow extends StatelessWidget {
  const _PresetRow({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) {
    final selected = controller.settings.duration;
    final isPreset = Settings.presets.contains(selected);

    return Wrap(
      alignment: WrapAlignment.center,
      spacing: 8,
      runSpacing: 8,
      children: [
        for (final preset in Settings.presets)
          _Chip(
            label: '${preset.inMinutes}',
            selected: preset == selected,
            onPressed: () => controller.setDuration(preset),
          ),
        _Chip(
          label: isPreset ? '···' : '${selected.inMinutes}',
          selected: !isPreset,
          onPressed: () async {
            final chosen = await showDurationSheet(context, selected);
            if (chosen != null) {
              await controller.setDuration(chosen);
            }
          },
        ),
      ],
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip({
    required this.label,
    required this.selected,
    required this.onPressed,
  });

  final String label;
  final bool selected;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => Material(
        color: selected ? JikidoColors.vermilion : JikidoColors.inkRaised,
        shape: const CircleBorder(),
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: onPressed,
          child: SizedBox(
            width: 46,
            height: 46,
            child: Center(
              child: Text(
                label,
                style: TextStyle(
                  color: selected ? JikidoColors.ink : JikidoColors.paper,
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                ),
              ),
            ),
          ),
        ),
      );
}

class _PrimaryButton extends StatelessWidget {
  const _PrimaryButton({required this.label, required this.onPressed});

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => SizedBox(
        width: 200,
        height: 54,
        child: FilledButton(
          onPressed: onPressed,
          style: FilledButton.styleFrom(
            backgroundColor: JikidoColors.vermilion,
            foregroundColor: JikidoColors.ink,
            shape: const StadiumBorder(),
          ),
          child: Text(
            label,
            style: const TextStyle(fontSize: 17, letterSpacing: 2),
          ),
        ),
      );
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) => Container(
        width: double.infinity,
        margin: const EdgeInsets.fromLTRB(16, 0, 16, 8),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: JikidoColors.vermilion.withValues(alpha: 0.15),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Text(
          message,
          style: const TextStyle(fontSize: 13, color: JikidoColors.paper),
        ),
      );
}

/// Formats a countdown as `mm:ss`, or `h:mm:ss` for the long sittings.
String formatRemaining(Duration remaining) {
  final seconds = remaining.inSeconds;
  final minutes = seconds ~/ 60;
  final secondsPart = (seconds % 60).toString().padLeft(2, '0');
  if (minutes < 60) {
    return '$minutes:$secondsPart';
  }
  final minutesPart = (minutes % 60).toString().padLeft(2, '0');
  return '${minutes ~/ 60}:$minutesPart:$secondsPart';
}
