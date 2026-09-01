import 'package:flutter/material.dart';

import '../alarm/exact_alarms.dart';
import '../bell.dart';
import '../settings.dart';
import '../sitting_controller.dart';
import 'theme.dart';

class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key, required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(
          title: const Text(
            'SETTINGS',
            style: TextStyle(letterSpacing: 4, fontSize: 13),
          ),
        ),
        body: ListenableBuilder(
          listenable: controller,
          builder: (context, _) => ListView(
            children: [
              const _SectionHeading('Bell'),
              RadioGroup<Bell>(
                groupValue: controller.settings.bell,
                onChanged: (chosen) {
                  if (chosen != null) {
                    controller.setBell(chosen);
                  }
                },
                child: Column(
                  children: [
                    for (final bell in Bell.values)
                      RadioListTile<Bell>(
                        value: bell,
                        activeColor: JikidoColors.vermilion,
                        title: Text(bell.label),
                        subtitle: Text(
                          bell.description,
                          style: const TextStyle(color: JikidoColors.faded),
                        ),
                        secondary: IconButton(
                          tooltip: 'Listen',
                          icon: const Icon(Icons.play_arrow),
                          onPressed: () async {
                            await controller.setBell(bell);
                            await controller.previewBell();
                          },
                        ),
                      ),
                  ],
                ),
              ),
              _BellSize(controller: controller),
              const Divider(),
              const _SectionHeading('Before sitting'),
              _PrepareTile(controller: controller),
              const Divider(),
              const _SectionHeading('While sitting'),
              SwitchListTile(
                value: controller.settings.keepScreenOn,
                onChanged: controller.setKeepScreenOn,
                activeThumbColor: JikidoColors.vermilion,
                title: const Text('Keep the screen on'),
                subtitle: const Text(
                  'Uses more battery, and is the surest way to have the '
                  'closing bell ring on time.',
                  style: TextStyle(color: JikidoColors.faded),
                ),
              ),
              const _ExactAlarmsTile(),
              const Divider(),
              const _SectionHeading('About'),
              const Padding(
                padding: EdgeInsets.fromLTRB(16, 4, 16, 32),
                child: Text(
                  'Three strikes of the bell open the sitting, and two close '
                  'it — the second stopped by laying the striker on the bowl '
                  'rather than letting it ring away, as in a zendo. The '
                  'countdown starts with the first strike, so the bell rings '
                  'out over the beginning of the period rather than before '
                  'it.\n\n'
                  'The bell plays on the alarm channel, so it is heard even '
                  'with the phone silenced. Its volume follows the alarm '
                  'volume — the opening bell tells you how loud the closing '
                  'one will be.',
                  style: TextStyle(color: JikidoColors.faded, height: 1.5),
                ),
              ),
            ],
          ),
        ),
      );
}

/// How large the bell is.
///
/// One slider, not two, because a real bell's pitch and how long it rings
/// are not independent: a bigger bowl sounds lower *and* longer. The
/// synthesizer holds the quality factor constant so this moves both at once,
/// the way casting a larger bell would.
class _BellSize extends StatelessWidget {
  const _BellSize({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) {
    final size = controller.settings.bellSize;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Text('Size'),
              const Spacer(),
              Text(
                _describe(size),
                style: const TextStyle(color: JikidoColors.faded),
              ),
            ],
          ),
          Slider(
            value: size,
            min: Settings.minimumBellSize,
            max: Settings.maximumBellSize,
            activeColor: JikidoColors.vermilion,
            onChanged: controller.setBellSize,
            // Struck when the finger lifts rather than continuously: a bell
            // that rang on every pixel of the drag would be unusable.
            onChangeEnd: (_) => controller.previewBell(),
          ),
          const Padding(
            padding: EdgeInsets.only(bottom: 8),
            child: Text(
              'A larger bell sounds lower and rings for longer, as a heavier '
              'casting does.',
              style: TextStyle(color: JikidoColors.faded, height: 1.4),
            ),
          ),
        ],
      ),
    );
  }

  static String _describe(double size) {
    if (size < 0.8) {
      return 'small';
    }
    if (size < 1.15) {
      return 'as cast';
    }
    if (size < 1.6) {
      return 'large';
    }
    return 'temple';
  }
}

/// The silence between pressing Sit and the opening bell.
class _PrepareTile extends StatelessWidget {
  const _PrepareTile({required this.controller});

  final SittingController controller;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Padding(
              padding: EdgeInsets.only(bottom: 12),
              child: Text('Settling time'),
            ),
            Wrap(
              spacing: 8,
              children: [
                for (final prepare in Settings.preparePresets)
                  ChoiceChip(
                    label: Text(_describe(prepare)),
                    selected: controller.settings.prepare == prepare,
                    selectedColor: JikidoColors.vermilion,
                    backgroundColor: JikidoColors.inkRaised,
                    showCheckmark: false,
                    onSelected: (_) => controller.setPrepare(prepare),
                  ),
              ],
            ),
            const Padding(
              padding: EdgeInsets.only(top: 12, bottom: 8),
              child: Text(
                'Silence between pressing Sit and the opening bell, to get '
                'settled. The sitting is timed from the bell, so this is not '
                'taken out of it.',
                style: TextStyle(color: JikidoColors.faded, height: 1.4),
              ),
            ),
          ],
        ),
      );

  static String _describe(Duration prepare) {
    if (prepare == Duration.zero) {
      return 'None';
    }
    if (prepare.inSeconds < 60) {
      return '${prepare.inSeconds}s';
    }
    return '${prepare.inMinutes} min';
  }
}

/// Offers to fix the exact-alarm permission, and says nothing at all when
/// there is nothing to fix.
class _ExactAlarmsTile extends StatefulWidget {
  const _ExactAlarmsTile();

  @override
  State<_ExactAlarmsTile> createState() => _ExactAlarmsTileState();
}

class _ExactAlarmsTileState extends State<_ExactAlarmsTile> {
  static const ExactAlarms _exactAlarms = ExactAlarms();

  bool _granted = true;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  Future<void> _refresh() async {
    final granted = await _exactAlarms.isGranted;
    if (mounted) {
      setState(() => _granted = granted);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!ExactAlarms.isRelevant || _granted) {
      return const SizedBox.shrink();
    }
    return ListTile(
      leading: const Icon(Icons.alarm_off, color: JikidoColors.vermilion),
      title: const Text('Allow alarms & reminders'),
      subtitle: const Text(
        'Without this permission Android may hold back the backup bell for '
        'a few minutes if Jikido is shut down mid-sitting.',
        style: TextStyle(color: JikidoColors.faded),
      ),
      onTap: () async {
        await _exactAlarms.openSettings();
        await _refresh();
      },
    );
  }
}

class _SectionHeading extends StatelessWidget {
  const _SectionHeading(this.label);

  final String label;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
        child: Text(
          label.toUpperCase(),
          style: const TextStyle(
            fontSize: 11,
            letterSpacing: 2,
            color: JikidoColors.faded,
          ),
        ),
      );
}
