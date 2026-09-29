import { createSignal } from 'solid-js';
import { describe, expect, it, vi } from 'vitest';
import type { ProjectsContext } from '../context/projects-context';
import type { ProjectAccess, ProjectDetail } from '../core/project';
import type { ProjectTaskDraft } from '../queries/project-task-cache';
import {
  projectCreateDestination,
  projectTaskComposer,
} from './project-task-composer';

function project(access: ProjectAccess = 'edit'): ProjectDetail {
  return {
    id: 'project-id',
    name: 'Launch',
    descriptionDocumentId: 'description',
    updatedAt: '',
    createdAt: '',
    ownerId: 'owner',
    memberIds: [],
    taskIds: [],
    access,
  };
}

type CreateProjectTask = ReturnType<
  ProjectsContext['createCommands']
>['createTask'];

const draft: ProjectTaskDraft = [
  'Title',
  'Body',
  [],
  new Map(),
  () => {},
  { shareWithTeam: true },
];

describe('project task composer', () => {
  it('adds the created task to the project and names it', async () => {
    const createTask = vi.fn<CreateProjectTask>(async () => null);
    const onCreated = vi.fn();
    const composer = projectTaskComposer(project(), createTask, onCreated);

    expect(composer.projectName).toBe('Launch');
    await expect(composer.createTask?.(...draft)).resolves.toBeNull();
    expect(createTask).toHaveBeenCalledExactlyOnceWith('project-id', ...draft);

    composer.onSuccess?.({
      documentId: 'task-id',
      title: 'Title',
      content: '',
    });
    expect(onCreated).toHaveBeenCalledOnce();
  });
});

describe('project create destination', () => {
  it('waits for the project and follows its name', () => {
    const [current, setCurrent] = createSignal<ProjectDetail>();
    const destination = projectCreateDestination(current, vi.fn(), vi.fn());

    expect(destination()).toBeUndefined();
    setCurrent(project());
    expect(destination()).toMatchObject({
      label: 'Launch',
      taskComposer: { projectName: 'Launch' },
    });
    setCurrent({ ...project(), name: 'Renamed' });
    expect(destination()?.label).toBe('Renamed');
  });

  it.each(['edit', 'owner'] as const)(
    'places tasks for a viewer with %s access',
    (access) => {
      const createTask = vi.fn<CreateProjectTask>(async () => null);
      const destination = projectCreateDestination(
        () => project(access),
        createTask,
        vi.fn()
      );
      void destination()?.taskComposer.createTask?.(...draft);
      expect(createTask).toHaveBeenCalledWith('project-id', ...draft);
    }
  );

  it.each(['view', 'comment'] as const)(
    'is withdrawn for a viewer with %s access',
    (access) => {
      const destination = projectCreateDestination(
        () => project(access),
        vi.fn(),
        vi.fn()
      );
      expect(destination()).toBeUndefined();
    }
  );
});
