import 'package:flutter/material.dart';

import '../alarm/exact_alarms.dart';
import '../bell.dart';
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
                  'Three strikes of the bell open the sitting and three '
                  'close it, as in a zendo. The countdown starts with the '
                  'first strike, so the bell rings out over the beginning of '
                  'the period rather than before it.\n\n'
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
