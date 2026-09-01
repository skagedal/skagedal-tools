import 'package:flutter/material.dart';

import '../settings.dart';
import 'theme.dart';

/// Asks for a sitting length that is not one of the presets.
Future<Duration?> showDurationSheet(BuildContext context, Duration initial) =>
    showModalBottomSheet<Duration>(
      context: context,
      backgroundColor: JikidoColors.inkRaised,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) => _DurationSheet(initial: initial),
    );

class _DurationSheet extends StatefulWidget {
  const _DurationSheet({required this.initial});

  final Duration initial;

  @override
  State<_DurationSheet> createState() => _DurationSheetState();
}

class _DurationSheetState extends State<_DurationSheet> {
  late int _minutes = Settings.clampDuration(widget.initial).inMinutes;

  static final int _min = Settings.minimumDuration.inMinutes;
  static final int _max = Settings.maximumDuration.inMinutes;

  void _nudge(int delta) {
    setState(() => _minutes = (_minutes + delta).clamp(_min, _max));
  }

  @override
  Widget build(BuildContext context) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'How long?',
                style: TextStyle(fontSize: 13, letterSpacing: 2,
                    color: JikidoColors.faded),
              ),
              const SizedBox(height: 16),
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  IconButton(
                    onPressed: _minutes > _min ? () => _nudge(-1) : null,
                    icon: const Icon(Icons.remove),
                  ),
                  SizedBox(
                    width: 120,
                    child: Text(
                      '$_minutes',
                      textAlign: TextAlign.center,
                      style: const TextStyle(
                        fontSize: 44,
                        fontWeight: FontWeight.w200,
                      ),
                    ),
                  ),
                  IconButton(
                    onPressed: _minutes < _max ? () => _nudge(1) : null,
                    icon: const Icon(Icons.add),
                  ),
                ],
              ),
              Slider(
                value: _minutes.toDouble(),
                min: _min.toDouble(),
                max: _max.toDouble(),
                activeColor: JikidoColors.vermilion,
                onChanged: (value) => setState(() => _minutes = value.round()),
              ),
              const SizedBox(height: 12),
              SizedBox(
                width: double.infinity,
                height: 48,
                child: FilledButton(
                  onPressed: () =>
                      Navigator.of(context).pop(Duration(minutes: _minutes)),
                  style: FilledButton.styleFrom(
                    backgroundColor: JikidoColors.vermilion,
                    foregroundColor: JikidoColors.ink,
                    shape: const StadiumBorder(),
                  ),
                  child: const Text('Choose'),
                ),
              ),
            ],
          ),
        ),
      );
}
